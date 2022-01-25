use std::{convert::Infallible, env::current_dir, path::Path, str::FromStr, time::Duration};

use clap::ArgMatches;
use miette::{IntoDiagnostic, Result};
use notify_rust::Notification;
use watchexec::{
	action::{Action, Outcome, PostSpawn, PreSpawn},
	command::Shell,
	config::RuntimeConfig,
	event::ProcessEnd,
	fs::Watcher,
	handler::SyncFnHandler,
	paths::summarise_events_to_env,
	signal::{process::SubSignal, source::MainSignal},
};

pub fn runtime(args: &ArgMatches<'static>) -> Result<RuntimeConfig> {
	let mut config = RuntimeConfig::default();

	config.command(
		args.values_of_lossy("command")
			.expect("(clap) Bug: command is not present")
			.iter(),
	);

	config.pathset(match args.values_of_os("paths") {
		Some(paths) => paths.map(|os| Path::new(os).to_owned()).collect(),
		None => vec![current_dir().into_diagnostic()?],
	});

	config.action_throttle(Duration::from_millis(
		args.value_of("debounce")
			.unwrap_or("100")
			.parse()
			.into_diagnostic()?,
	));

	if let Some(interval) = args.value_of("poll") {
		config.file_watcher(Watcher::Poll(Duration::from_millis(
			interval.parse().into_diagnostic()?,
		)));
	}

	if args.is_present("no-process-group") {
		config.command_grouped(false);
	}

	config.command_shell(if args.is_present("no-shell") {
		Shell::None
	} else if let Some(s) = args.value_of("shell") {
		if s.eq_ignore_ascii_case("powershell") {
			Shell::Powershell
		} else if s.eq_ignore_ascii_case("none") {
			Shell::None
		} else if s.eq_ignore_ascii_case("cmd") {
			cmd_shell(s.into())
		} else {
			Shell::Unix(s.into())
		}
	} else {
		default_shell()
	});

	let clear = args.is_present("clear");
	let notif = args.is_present("notif");
	let mut on_busy = args
		.value_of("on-busy-update")
		.unwrap_or("queue")
		.to_owned();

	if args.is_present("restart") {
		on_busy = "restart".into();
	}

	if args.is_present("watch-when-idle") {
		on_busy = "do-nothing".into();
	}

	let mut signal = args
		.value_of("signal")
		.map(SubSignal::from_str)
		.transpose()
		.into_diagnostic()?
		.unwrap_or(SubSignal::Terminate);

	if args.is_present("kill") {
		signal = SubSignal::ForceStop;
	}

	let print_events = args.is_present("print-events");
	let once = args.is_present("once");

	config.on_action(move |action: Action| {
		let fut = async { Ok::<(), Infallible>(()) };

		if print_events {
			for (n, event) in action.events.iter().enumerate() {
				eprintln!("[EVENT {}] {}", n, event);
			}
		}

		if once {
			action.outcome(Outcome::both(Outcome::Start, Outcome::wait(Outcome::Exit)));
			return fut;
		}

		let signals: Vec<MainSignal> = action.events.iter().flat_map(|e| e.signals()).collect();
		let has_paths = action
			.events
			.iter()
			.flat_map(|e| e.paths())
			.next()
			.is_some();

		if signals.contains(&MainSignal::Terminate) {
			action.outcome(Outcome::both(Outcome::Stop, Outcome::Exit));
			return fut;
		}

		if signals.contains(&MainSignal::Interrupt) {
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

			let completion = action.events.iter().flat_map(|e| e.completions()).next();
			if let Some(status) = completion {
				let (msg, printit) = match status {
					Some(ProcessEnd::ExitError(code)) => {
						(format!("Command exited with {}", code), true)
					}
					Some(ProcessEnd::ExitSignal(sig)) => {
						(format!("Command killed by {:?}", sig), true)
					}
					Some(ProcessEnd::ExitStop(sig)) => {
						(format!("Command stopped by {:?}", sig), true)
					}
					Some(ProcessEnd::Continued) => ("Command continued".to_string(), true),
					Some(ProcessEnd::Exception(ex)) => {
						(format!("Command ended by exception {:#x}", ex), true)
					}
					Some(ProcessEnd::Success) => ("Command was successful".to_string(), false),
					None => ("Command completed".to_string(), false),
				};

				if printit {
					eprintln!("[[{}]]", msg);
				}

				if notif {
					Notification::new()
						.summary("Watchexec: command ended")
						.body(&msg)
						.show()
						.map(drop)
						.unwrap_or_else(|err| {
							eprintln!("[[Failed to send desktop notification: {}]]", err);
						});
				}

				action.outcome(Outcome::DoNothing);
				return fut;
			}
		}

		let when_running = match (clear, on_busy.as_str()) {
			(_, "do-nothing") => Outcome::DoNothing,
			(true, "restart") => {
				Outcome::both(Outcome::Stop, Outcome::both(Outcome::Clear, Outcome::Start))
			}
			(false, "restart") => Outcome::both(Outcome::Stop, Outcome::Start),
			(_, "signal") => Outcome::Signal(signal),
			(true, "queue") => Outcome::wait(Outcome::both(Outcome::Clear, Outcome::Start)),
			(false, "queue") => Outcome::wait(Outcome::Start),
			_ => Outcome::DoNothing,
		};

		let when_idle = if clear {
			Outcome::both(Outcome::Clear, Outcome::Start)
		} else {
			Outcome::Start
		};

		action.outcome(Outcome::if_running(when_running, when_idle));

		fut
	});

	let no_env = args.is_present("no-environment");
	config.on_pre_spawn(move |prespawn: PreSpawn| async move {
		if !no_env {
			let envs = summarise_events_to_env(prespawn.events.iter());
			if let Some(mut command) = prespawn.command().await {
				for (k, v) in envs {
					command.env(format!("WATCHEXEC_{}_PATH", k), v);
				}
			}
		}

		Ok::<(), Infallible>(())
	});

	config.on_post_spawn(SyncFnHandler::from(move |postspawn: PostSpawn| {
		if notif {
			Notification::new()
				.summary("Watchexec: change detected")
				.body(&format!("Running `{}`", postspawn.command.join(" ")))
				.show()
				.map(drop)
				.unwrap_or_else(|err| {
					eprintln!("[[Failed to send desktop notification: {}]]", err);
				});
		}

		Ok::<(), Infallible>(())
	}));

	Ok(config)
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
