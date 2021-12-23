use std::{convert::Infallible, env::current_dir, path::Path, str::FromStr, time::Duration};

use clap::ArgMatches;
use miette::{IntoDiagnostic, Result};
use watchexec::{
	action::{Action, Outcome},
	command::Shell,
	config::RuntimeConfig,
	event::ProcessEnd,
	fs::Watcher,
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
				match status {
					Some(ProcessEnd::ExitError(code)) => {
						eprintln!("[Command exited with {}]", code);
					}
					Some(ProcessEnd::ExitSignal(sig)) => {
						eprintln!("[Command killed by {:?}]", sig);
					}
					Some(ProcessEnd::ExitStop(sig)) => {
						eprintln!("[Command stopped by {:?}]", sig);
					}
					Some(ProcessEnd::Continued) => {
						eprintln!("[Command continued]");
					}
					Some(ProcessEnd::Exception(ex)) => {
						eprintln!("[Command ended by exception {:#x}]", ex);
					}
					_ => {}
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

	// TODO: pre-command (environment vars)

	Ok(config)
}

// until 2.0, then Powershell
#[cfg(windows)]
fn default_shell() -> Shell {
	Shell::Cmd
}

#[cfg(not(windows))]
fn default_shell() -> Shell {
	Shell::default()
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
