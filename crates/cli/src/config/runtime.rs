use std::{
	collections::HashMap, convert::Infallible, env::current_dir, ffi::OsString, fs::File,
	process::Stdio,
};

use miette::{miette, IntoDiagnostic, Result};
use notify_rust::Notification;
use tracing::{debug, debug_span, error};
use watchexec::{
	action::{Action, Outcome, PostSpawn, PreSpawn},
	command::{Command, Shell},
	config::RuntimeConfig,
	error::RuntimeError,
	fs::Watcher,
	handler::SyncFnHandler,
};
use watchexec_events::{Event, Keyboard, ProcessEnd, Tag};
use watchexec_signals::Signal;

use crate::args::{Args, ClearMode, EmitEvents, OnBusyUpdate};
use crate::state::State;

pub fn runtime(args: &Args, state: &State) -> Result<RuntimeConfig> {
	let _span = debug_span!("args-runtime").entered();
	let mut config = RuntimeConfig::default();

	config.command(interpret_command_args(args)?);

	config.pathset(if args.paths.is_empty() {
		vec![current_dir().into_diagnostic()?]
	} else {
		args.paths.clone()
	});

	config.action_throttle(args.debounce.0);
	config.command_grouped(!args.no_process_group);
	config.keyboard_emit_eof(args.stdin_quit);

	if let Some(interval) = args.poll {
		config.file_watcher(Watcher::Poll(interval.0));
	}

	let clear = args.screen_clear;
	let notif = args.notify;
	let on_busy = args.on_busy_update;

	let signal = args.signal;
	let stop_signal = args.stop_signal;
	let stop_timeout = args.stop_timeout.0;

	let print_events = args.print_events;
	let once = args.once;
	let delay_run = args.delay_run.map(|ts| ts.0);

	config.on_action(move |action: Action| {
		let fut = async { Ok::<(), Infallible>(()) };

		if print_events {
			for (n, event) in action.events.iter().enumerate() {
				eprintln!("[EVENT {n}] {event}");
			}
		}

		if once {
			action.outcome(Outcome::both(
				if let Some(delay) = &delay_run {
					Outcome::both(Outcome::Sleep(*delay), Outcome::Start)
				} else {
					Outcome::Start
				},
				Outcome::wait(Outcome::Exit),
			));
			return fut;
		}

		let signals: Vec<Signal> = action.events.iter().flat_map(Event::signals).collect();
		let has_paths = action.events.iter().flat_map(Event::paths).next().is_some();

		if signals.contains(&Signal::Terminate) {
			action.outcome(Outcome::both(Outcome::Stop, Outcome::Exit));
			return fut;
		}

		if signals.contains(&Signal::Interrupt) {
			action.outcome(Outcome::both(Outcome::Stop, Outcome::Exit));
			return fut;
		}

		let is_keyboard_eof = action
			.events
			.iter()
			.any(|e| e.tags.contains(&Tag::Keyboard(Keyboard::Eof)));

		if is_keyboard_eof {
			action.outcome(Outcome::both(Outcome::Stop, Outcome::Exit));
			return fut;
		}

		if !has_paths {
			if !signals.is_empty() {
				let mut out = Outcome::DoNothing;
				for sig in signals {
					out = Outcome::both(out, Outcome::Signal(sig));
				}

				action.outcome(out);
				return fut;
			}

			let completion = action.events.iter().flat_map(Event::completions).next();
			if let Some(status) = completion {
				let (msg, printit) = match status {
					Some(ProcessEnd::ExitError(code)) => {
						(format!("Command exited with {code}"), true)
					}
					Some(ProcessEnd::ExitSignal(sig)) => {
						(format!("Command killed by {sig:?}"), true)
					}
					Some(ProcessEnd::ExitStop(sig)) => {
						(format!("Command stopped by {sig:?}"), true)
					}
					Some(ProcessEnd::Continued) => ("Command continued".to_string(), true),
					Some(ProcessEnd::Exception(ex)) => {
						(format!("Command ended by exception {ex:#x}"), true)
					}
					Some(ProcessEnd::Success) => ("Command was successful".to_string(), false),
					None => ("Command completed".to_string(), false),
				};

				if printit {
					eprintln!("[[{msg}]]");
				}

				if notif {
					Notification::new()
						.summary("Watchexec: command ended")
						.body(&msg)
						.show()
						.map_or_else(
							|err| {
								eprintln!("[[Failed to send desktop notification: {err}]]");
							},
							drop,
						);
				}

				action.outcome(Outcome::DoNothing);
				return fut;
			}
		}

		let start = if let Some(mode) = clear {
			Outcome::both(
				match mode {
					ClearMode::Clear => Outcome::Clear,
					ClearMode::Reset => Outcome::Reset,
				},
				Outcome::Start,
			)
		} else {
			Outcome::Start
		};

		let start = if let Some(delay) = &delay_run {
			Outcome::both(Outcome::Sleep(*delay), start)
		} else {
			start
		};

		let when_idle = start.clone();
		let when_running = match on_busy {
			OnBusyUpdate::Restart => Outcome::both(
				Outcome::both(
					Outcome::Signal(stop_signal.unwrap_or(Signal::Terminate)),
					Outcome::wait_timeout(stop_timeout, Outcome::Stop),
				),
				start,
			),
			OnBusyUpdate::Signal => {
				Outcome::Signal(stop_signal.or(signal).unwrap_or(Signal::Terminate))
			}
			OnBusyUpdate::Queue => Outcome::wait(start),
			OnBusyUpdate::DoNothing => Outcome::DoNothing,
		};

		action.outcome(Outcome::if_running(when_running, when_idle));

		fut
	});

	let mut add_envs = HashMap::new();
	// TODO: move to args?
	for pair in &args.env {
		if let Some((k, v)) = pair.split_once('=') {
			add_envs.insert(k.to_owned(), OsString::from(v));
		} else {
			return Err(miette!("{pair} is not in key=value format"));
		}
	}
	debug!(
		?add_envs,
		"additional environment variables to add to command"
	);

	let workdir = args.workdir.clone();

	let emit_events_to = args.emit_events_to;
	let emit_file = state.emit_file.clone();
	config.on_pre_spawn(move |prespawn: PreSpawn| {
		use crate::emits::*;

		let workdir = workdir.clone();
		let mut add_envs = add_envs.clone();
		let mut stdin = None;

		match emit_events_to {
			EmitEvents::Environment => {
				add_envs.extend(emits_to_environment(&prespawn.events));
			}
			EmitEvents::Stdin => match emits_to_file(&emit_file, &prespawn.events)
				.and_then(|path| File::open(path).into_diagnostic())
			{
				Ok(file) => {
					stdin.replace(Stdio::from(file));
				}
				Err(err) => {
					error!("Failed to write events to stdin, continuing without it: {err}");
				}
			},
			EmitEvents::File => match emits_to_file(&emit_file, &prespawn.events) {
				Ok(path) => {
					add_envs.insert("WATCHEXEC_EVENTS_FILE".into(), path.into());
				}
				Err(err) => {
					error!("Failed to write WATCHEXEC_EVENTS_FILE, continuing without it: {err}");
				}
			},
			EmitEvents::JsonStdin => match emits_to_json_file(&emit_file, &prespawn.events)
				.and_then(|path| File::open(path).into_diagnostic())
			{
				Ok(file) => {
					stdin.replace(Stdio::from(file));
				}
				Err(err) => {
					error!("Failed to write events to stdin, continuing without it: {err}");
				}
			},
			EmitEvents::JsonFile => match emits_to_json_file(&emit_file, &prespawn.events) {
				Ok(path) => {
					add_envs.insert("WATCHEXEC_EVENTS_FILE".into(), path.into());
				}
				Err(err) => {
					error!("Failed to write WATCHEXEC_EVENTS_FILE, continuing without it: {err}");
				}
			},
			EmitEvents::None => {}
		}

		async move {
			if !add_envs.is_empty() || workdir.is_some() || stdin.is_some() {
				if let Some(mut command) = prespawn.command().await {
					for (k, v) in add_envs {
						debug!(?k, ?v, "inserting environment variable");
						command.env(k, v);
					}

					if let Some(ref workdir) = workdir {
						debug!(?workdir, "set command workdir");
						command.current_dir(workdir);
					}

					if let Some(stdin) = stdin {
						debug!("set command stdin");
						command.stdin(stdin);
					}
				}
			}

			Ok::<(), Infallible>(())
		}
	});

	config.on_post_spawn(SyncFnHandler::from(move |postspawn: PostSpawn| {
		if notif {
			Notification::new()
				.summary("Watchexec: change detected")
				.body(&format!("Running {}", postspawn.command))
				.show()
				.map_or_else(
					|err| {
						eprintln!("[[Failed to send desktop notification: {err}]]");
					},
					drop,
				);
		}

		Ok::<(), Infallible>(())
	}));

	Ok(config)
}

fn interpret_command_args(args: &Args) -> Result<Command> {
	let mut cmd = args.command.clone();
	if cmd.is_empty() {
		panic!("(clap) Bug: command is not present");
	}

	Ok(if args.no_shell || args.no_shell_long {
		Command::Exec {
			prog: cmd.remove(0),
			args: cmd,
		}
	} else {
		let (shell, shopts) = if let Some(s) = &args.shell {
			if s.is_empty() {
				return Err(RuntimeError::CommandShellEmptyShell).into_diagnostic();
			} else if s.eq_ignore_ascii_case("powershell") {
				(Shell::Powershell, Vec::new())
			} else if s.eq_ignore_ascii_case("none") {
				return Ok(Command::Exec {
					prog: cmd.remove(0),
					args: cmd,
				});
			} else if s.eq_ignore_ascii_case("cmd") {
				(cmd_shell(s.into()), Vec::new())
			} else {
				let sh = s.split_ascii_whitespace().collect::<Vec<_>>();

				// UNWRAP: checked by first if branch
				#[allow(clippy::unwrap_used)]
				let (shprog, shopts) = sh.split_first().unwrap();

				(
					Shell::Unix((*shprog).to_string()),
					shopts.iter().map(|s| (*s).to_string()).collect(),
				)
			}
		} else {
			(default_shell(), Vec::new())
		};

		Command::Shell {
			shell,
			args: shopts,
			command: cmd.join(" "),
		}
	})
}

// until 2.0, then Powershell
#[cfg(windows)]
fn default_shell() -> Shell {
	Shell::Cmd
}

#[cfg(not(windows))]
fn default_shell() -> Shell {
	Shell::Unix("sh".to_string())
}

// because Shell::Cmd is only on windows
#[cfg(windows)]
fn cmd_shell(_: String) -> Shell {
	Shell::Cmd
}

#[cfg(not(windows))]
fn cmd_shell(s: String) -> Shell {
	Shell::Unix(s)
}
