use std::{
	collections::HashMap, convert::Infallible, env::current_dir, ffi::OsString, path::Path,
	str::FromStr, string::ToString, time::Duration,
};

use clap::ArgMatches;
use miette::{miette, IntoDiagnostic, Result};
use notify_rust::Notification;
use tracing::{debug, debug_span};
use watchexec::{
	action::{Action, Outcome, PostSpawn, PreSpawn},
	command::{Command, Shell},
	config::RuntimeConfig,
	error::RuntimeError,
	event::{Event, ProcessEnd, Tag},
	fs::Watcher,
	handler::SyncFnHandler,
	keyboard::Keyboard,
	paths::summarise_events_to_env,
	signal::{process::SubSignal, source::MainSignal},
};

pub fn runtime(args: &ArgMatches) -> Result<RuntimeConfig> {
	let _span = debug_span!("args-runtime").entered();
	let mut config = RuntimeConfig::default();

	config.command(interpret_command_args(args)?);

	config.pathset(match args.values_of_os("paths") {
		Some(paths) => paths.map(|os| Path::new(os).to_owned()).collect(),
		None => vec![current_dir().into_diagnostic()?],
	});

	config.action_throttle(Duration::from_millis(
		args.value_of("debounce")
			.unwrap_or("50")
			.parse()
			.into_diagnostic()?,
	));

	config.keyboard_emit_eof(args.is_present("stdin-quit"));

	if let Some(interval) = args.value_of("poll") {
		config.file_watcher(Watcher::Poll(Duration::from_millis(
			interval.parse().into_diagnostic()?,
		)));
	}

	if args.is_present("no-process-group") {
		config.command_grouped(false);
	}

	let clear = args.is_present("clear");
	let notif = args.is_present("notif");
	let on_busy = if args.is_present("restart") {
		"restart"
	} else if args.is_present("watch-when-idle") {
		"do-nothing"
	} else {
		args.value_of("on-busy-update").unwrap_or("queue")
	}
	.to_owned();

	let signal = if args.is_present("kill") {
		Some(SubSignal::ForceStop)
	} else {
		args.value_of("signal")
			.map(SubSignal::from_str)
			.transpose()
			.into_diagnostic()?
	};

	let print_events = args.is_present("print-events");
	let once = args.is_present("once");
	let delay_run = args
		.value_of("delay-run")
		.map(u64::from_str)
		.transpose()
		.into_diagnostic()?
		.map(Duration::from_secs);

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

		let signals: Vec<MainSignal> = action.events.iter().flat_map(Event::signals).collect();
		let has_paths = action.events.iter().flat_map(Event::paths).next().is_some();

		if signals.contains(&MainSignal::Terminate) {
			action.outcome(Outcome::both(Outcome::Stop, Outcome::Exit));
			return fut;
		}

		if signals.contains(&MainSignal::Interrupt) {
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
					out = Outcome::both(out, Outcome::Signal(sig.into()));
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

		let start = if clear {
			Outcome::both(Outcome::Clear, Outcome::Start)
		} else {
			Outcome::Start
		};

		let start = if let Some(delay) = &delay_run {
			Outcome::both(Outcome::Sleep(*delay), start)
		} else {
			start
		};

		let when_idle = start.clone();
		let when_running = match on_busy.as_str() {
			"restart" => Outcome::both(
				if let Some(sig) = signal {
					Outcome::both(
						Outcome::Signal(sig),
						Outcome::both(Outcome::Sleep(Duration::from_secs(60)), Outcome::Stop),
					)
				} else {
					Outcome::Stop
				},
				start,
			),
			"signal" => Outcome::Signal(signal.unwrap_or(SubSignal::Terminate)),
			"queue" => Outcome::wait(start),
			// "do-nothing" => Outcome::DoNothing,
			_ => Outcome::DoNothing,
		};

		action.outcome(Outcome::if_running(when_running, when_idle));

		fut
	});

	let mut add_envs = HashMap::new();
	for pair in args.values_of("command-env").unwrap_or_default() {
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

	let workdir = args
		.value_of_os("command-workdir")
		.map(|wkd| Path::new(wkd).to_owned());

	let no_env = args.is_present("no-environment");
	config.on_pre_spawn(move |prespawn: PreSpawn| {
		let add_envs = add_envs.clone();
		let workdir = workdir.clone();
		async move {
			if !no_env || !add_envs.is_empty() || workdir.is_some() {
				if let Some(mut command) = prespawn.command().await {
					let mut envs = add_envs.clone();

					if !no_env {
						envs.extend(
							summarise_events_to_env(prespawn.events.iter())
								.into_iter()
								.map(|(k, v)| (format!("WATCHEXEC_{k}_PATH"), v)),
						);
					}

					for (k, v) in envs {
						debug!(?k, ?v, "inserting environment variable");
						command.env(k, v);
					}

					if let Some(ref workdir) = workdir {
						debug!(?workdir, "set command workdir");
						command.current_dir(workdir);
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

fn interpret_command_args(args: &ArgMatches) -> Result<Command> {
	let mut cmd = args
		.values_of("command")
		.expect("(clap) Bug: command is not present")
		.map(ToString::to_string)
		.collect::<Vec<_>>();

	Ok(if args.is_present("no-shell") {
		Command::Exec {
			prog: cmd.remove(0),
			args: cmd,
		}
	} else {
		let (shell, shopts) = if let Some(s) = args.value_of("shell") {
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
