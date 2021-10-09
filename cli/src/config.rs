use std::{
	convert::Infallible, env::current_dir, io::stderr, path::Path, str::FromStr, sync::Arc,
	time::Duration,
};

use clap::ArgMatches;
use color_eyre::eyre::{eyre, Result};
use watchexec::{
	action::{Action, Outcome, Signal},
	command::Shell,
	config::{InitConfig, RuntimeConfig},
	filter::tagged::TaggedFilterer,
	fs::Watcher,
	handler::PrintDisplay,
	signal::Signal as InputSignal,
};

pub fn new(args: &ArgMatches<'static>) -> Result<(InitConfig, RuntimeConfig, Arc<TaggedFilterer>)> {
	let r = runtime(args)?;
	Ok((init(args)?, r.0, r.1))
}

fn init(_args: &ArgMatches<'static>) -> Result<InitConfig> {
	let mut config = InitConfig::default();
	config.on_error(PrintDisplay(stderr()));
	Ok(config)
}

fn runtime(args: &ArgMatches<'static>) -> Result<(RuntimeConfig, Arc<TaggedFilterer>)> {
	let mut config = RuntimeConfig::default();

	config.command(
		args.values_of_lossy("command")
			.ok_or_else(|| eyre!("(clap) Bug: command is not present"))?
			.iter(),
	);

	config.pathset(match args.values_of_os("paths") {
		Some(paths) => paths.map(|os| Path::new(os).to_owned()).collect(),
		None => vec![current_dir()?],
	});

	config.action_throttle(Duration::from_millis(
		args.value_of("debounce").unwrap_or("100").parse()?,
	));

	if let Some(interval) = args.value_of("poll") {
		config.file_watcher(Watcher::Poll(Duration::from_millis(interval.parse()?)));
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
		.map(|s| Signal::from_str(s))
		.transpose()?
		.unwrap_or(Signal::SIGTERM);

	if args.is_present("kill") {
		signal = Signal::SIGKILL;
	}

	let print_events = args.is_present("print-events");
	let once = args.is_present("once");

	let filterer = TaggedFilterer::new(".", ".")?;
	config.filterer(filterer.clone());

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

		let signals: Vec<InputSignal> = action.events.iter().flat_map(|e| e.signals()).collect();
		let has_paths = action
			.events
			.iter()
			.flat_map(|e| e.paths())
			.next()
			.is_some();

		if signals.contains(&InputSignal::Terminate) {
			action.outcome(Outcome::both(Outcome::Stop, Outcome::Exit));
			return fut;
		}

		if signals.contains(&InputSignal::Interrupt) {
			action.outcome(Outcome::both(Outcome::Stop, Outcome::Exit));
			return fut;
		}

		if !has_paths {
			if !signals.is_empty() {
				let mut out = Outcome::DoNothing;
				for sig in signals {
					out = Outcome::both(
						out,
						Outcome::Signal(match sig {
							InputSignal::Hangup => Signal::SIGHUP,
							InputSignal::Interrupt => Signal::SIGINT,
							InputSignal::Quit => Signal::SIGQUIT,
							InputSignal::Terminate => Signal::SIGTERM,
							InputSignal::User1 => Signal::SIGUSR1,
							InputSignal::User2 => Signal::SIGUSR2,
						}),
					);
				}

				action.outcome(out);
				return fut;
			}

			let completion = action.events.iter().flat_map(|e| e.completions()).next();
			if let Some(status) = completion {
				match status {
					Some(ex) if !ex.success() => {
						eprintln!("[Command exited with {}]", ex);
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

	Ok((config, filterer))
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
