use std::{
	borrow::Cow,
	collections::HashMap,
	convert::Infallible,
	env::current_dir,
	ffi::{OsStr, OsString},
	fs::File,
	process::Stdio,
};

use miette::{miette, IntoDiagnostic, Result};
use notify_rust::Notification;
use tracing::{debug, debug_span, error};
use watchexec::{
	action::{Action, /*Outcome,*/ PostSpawn, PreSpawn},
	command::{Command, Isolation, Program, Shell},
	config::RuntimeConfig,
	error::RuntimeError,
	fs::Watcher,
	handler::SyncFnHandler,
};

use watchexec_events::{Event, Keyboard, ProcessEnd, Tag};
use watchexec_signals::Signal;

use crate::args::{Args, /*ClearMode,*/ EmitEvents /*OnBusyUpdate*/};
use crate::state::State;

pub fn runtime(args: &Args, state: &State) -> Result<RuntimeConfig> {
	let _span = debug_span!("args-runtime").entered();
	let mut config = RuntimeConfig::default();

	let mut command = Some(interpret_command_args(args)?);

	config.pathset(if args.paths.is_empty() {
		vec![current_dir().into_diagnostic()?]
	} else {
		args.paths.clone()
	});

	config.action_throttle(args.debounce.0);
	config.keyboard_emit_eof(args.stdin_quit);

	if let Some(interval) = args.poll {
		config.file_watcher(Watcher::Poll(interval.0));
	}

	let notif = args.notify;
	/*
	let clear = args.screen_clear;
	let on_busy = args.on_busy_update;

	let signal = args.signal;
	let stop_signal = args.stop_signal;
	let stop_timeout = args.stop_timeout.0;

	let print_events = args.print_events;
	let once = args.once;
	let delay_run = args.delay_run.map(|ts| ts.0);
	*/

	config.on_action(move |_action: Action| {
		let fut = async { Ok::<(), Infallible>(()) };

		/*
		// starts the command for the first time.
		// TODO(Félix) is this a valid way of spawning the command?
		// i think this means, if the command is spawned for the first time it will be started even
		// if there is a Terminate signal in the events of the Action!
		if let Some(command) = command.take() {
			_ = action.blocking_start_command(vec![command], watchexec::action::EventSet::All);
		}

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
			OnBusyUpdate::Restart if cfg!(windows) => Outcome::both(Outcome::Stop, start),
			OnBusyUpdate::Restart => Outcome::both(
				Outcome::both(
					Outcome::Signal(stop_signal.unwrap_or(Signal::Terminate)),
					Outcome::wait_timeout(stop_timeout, Outcome::Stop),
				),
				start,
			),
			OnBusyUpdate::Signal if cfg!(windows) => Outcome::Stop,
			OnBusyUpdate::Signal => {
				Outcome::Signal(stop_signal.or(signal).unwrap_or(Signal::Terminate))
			}
			OnBusyUpdate::Queue => Outcome::wait(start),
			OnBusyUpdate::DoNothing => Outcome::DoNothing,
		};

		action.outcome(Outcome::if_running(when_running, when_idle));

		*/
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
				.body(&format!("Running {}", postspawn.program))
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

	let shell = match if args.no_shell || args.no_shell_long {
		None
	} else {
		args.shell.as_deref().or(Some("default"))
	} {
		Some("") => return Err(RuntimeError::CommandShellEmptyShell).into_diagnostic(),

		Some("none") | None => None,

		#[cfg(windows)]
		Some("default") | Some("cmd") | Some("cmd.exe") | Some("CMD") | Some("CMD.EXE") => {
			Some(Shell::cmd())
		}

		#[cfg(not(windows))]
		Some("default") => Some(Shell::new("sh")),

		#[cfg(windows)]
		Some("powershell") => Some(Shell::new(available_powershell())),

		Some(other) => {
			let sh = other.split_ascii_whitespace().collect::<Vec<_>>();

			// UNWRAP: checked by Some("")
			#[allow(clippy::unwrap_used)]
			let (shprog, shopts) = sh.split_first().unwrap();

			Some(Shell {
				prog: shprog.into(),
				options: shopts.iter().map(|s| (*s).to_string()).collect(),
				program_option: Some(Cow::Borrowed(OsStr::new("-c"))),
			})
		}
	};

	let program = if let Some(shell) = shell {
		Program::Shell {
			shell,
			command: cmd.join(" "),
			args: Vec::new(),
		}
	} else {
		Program::Exec {
			prog: cmd.remove(0).into(),
			args: cmd,
		}
	};

	let mut command = Command::from(program);
	if !args.no_process_group {
		command.isolation = Isolation::Grouped;
	}
	Ok(command)
}

#[cfg(windows)]
fn available_powershell() -> String {
	todo!("figure out if powershell.exe is available, and use that, otherwise use pwsh.exe")
}
