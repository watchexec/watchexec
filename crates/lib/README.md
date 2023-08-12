[![Crates.io page](https://badgen.net/crates/v/watchexec)](https://crates.io/crates/watchexec)
[![API Docs](https://docs.rs/watchexec/badge.svg)][docs]
[![Crate license: Apache 2.0](https://badgen.net/badge/license/Apache%202.0)][license]
[![CI status](https://github.com/watchexec/watchexec/actions/workflows/check.yml/badge.svg)](https://github.com/watchexec/watchexec/actions/workflows/check.yml)

# Watchexec library

_The library which powers [Watchexec CLI](https://watchexec.github.io) and other tools._

- **[API documentation][docs]**.
- Licensed under [Apache 2.0][license].
- Status: maintained.

[docs]: https://docs.rs/watchexec
[license]: ../../LICENSE


## Quick start

```rust ,no_run
use std::convert::Infallible;
use miette::{IntoDiagnostic, Result};
use watchexec_events::{Event, Priority};
use watchexec_signals::Signal;
use watchexec::{
	action::{Action, EventSet, Outcome},
	command::{Program, Shell},
	config::{InitConfig, RuntimeConfig},
	handler::{sync, PrintDebug},
	Watchexec,
};

#[tokio::main]
async fn main() -> Result<()> {
	let mut init = InitConfig::default();
	init.on_error(PrintDebug(std::io::stderr()));
	// ^ this is okay to start with but quickly gets much too verbose,
	//   substitute your own error handling appropriate for your app!

	// define a simple initial configuration
	let mut runtime = RuntimeConfig::default();
	runtime.on_action(sync(|action: Action| -> Result<(), Infallible> {
		let id = action.create(Program::Shell {
			shell: Shell::new("bash"),
			command: "
				echo 'Hello world';
				trap INT 'echo Not quitting yet!';
				read
			".into(),
			args: Vec::new(),
		}.into());
		action.apply(id, Outcome::Start, EventSet::All);
		Ok(())
	}));

	// Initialise Watchexec
	let we = Watchexec::new(init, runtime.clone())?;
	// start the engine
	let main = we.main();

	// send an event to start
	we.send_event(Event::default(), Priority::Urgent).await.unwrap();
	// ^ this will cause the on_action handler we've defined above to run,
	//   creating and starting our little bash program

	// now we change what the action does:
	runtime.on_action(sync(|action: Action| -> Result<(), Infallible> {
		// if we get Ctrl-C on the Watchexec instance, we quit
		if action.signals().any(|sig| sig == Signal::Interrupt) {
			action.quit();
			return Ok(());
		}

		// if the action was triggered by file events,
		// send a SIGINT to the program
		if action.paths().next().is_some() {
			// watchexec can manage ("supervise") more than one program;
			// here we only have one but it's simpler to just iterate:
			for id in action.supervisors.iter().copied() {
				action.apply(id, Outcome::Signal(Signal::Interrupt), EventSet::All);
				// when there's more than one program, the EventSet argument ^
				// lets you indicate which subset of events influenced the
				// outcome you're applying to a particular program
			}
		}

		// if the program stopped, print a message and start it again
		if let Some(completion) = action.completions().next() {
			eprintln!("[Program stopped! {completion:?}]\n[Restarting...]");
			for id in action.supervisors.iter().copied() {
				action.apply(
					id,
					// outcomes are not applied immediately, so the program might already
					// have restarted by the time Watchexec gets to processing this outcome.
					// just in case, tell Watchexec to do nothing if the program is running:
					Outcome::if_running(Outcome::DoNothing, Outcome::Start),
					EventSet::All,
				);
			}
		}

		Ok(())
	}));

	// watch all files in the current directory:
	runtime.pathset(vec!["."]);

	// apply the new configuration!
	we.reconfigure(runtime)?;

	// now keep running until Watchexec quits
	let _ = main.await.into_diagnostic()?;
	Ok(())
}
```


## Kitchen sink

The library also exposes a number of components which are available to make your own tool, or to
make anything else you may want:

- **[Command handling](https://docs.rs/watchexec/2/watchexec/command/index.html)**, to
  build a command with an arbitrary shell, deal with grouped and ungrouped processes the same way,
  and supervise a process while also listening for & acting on interventions such as sending signals.

- **Event sources**: [Filesystem](https://docs.rs/watchexec/2/watchexec/fs/index.html),
  [Signals](https://docs.rs/watchexec/2/watchexec/signal/index.html),
  [Keyboard](https://docs.rs/watchexec/2/watchexec/keyboard/index.html),
  (more to come).

- Finding **[a common prefix](https://docs.rs/watchexec/2/watchexec/paths/fn.common_prefix.html)**
  of a set of paths.

- And [more][docs]!

Filterers are split into their own crates, so they can be evolved independently:

- The **[Globset](https://docs.rs/watchexec-filterer-globset) filterer** implements the default
  Watchexec filter, and mimics the pre-1.18 behaviour as much as possible.

- The **[Tagged](https://docs.rs/watchexec-filterer-tagged) filterer** is an experiment in creating
  a more powerful filtering solution, which can operate on every part of events, not just their
  paths.

- The **[Ignore](https://docs.rs/watchexec-filterer-ignore) filterer** implements ignore-file
  semantics, and especially supports _trees_ of ignore files. It is used as a subfilterer in both
  of the main filterers above.

There are also separate, standalone crates used to build Watchexec which you can tap into:

- **[ClearScreen](https://docs.rs/clearscreen)** makes clearing the terminal screen in a
  cross-platform way easy by default, and provides advanced options to fit your usecase.

- **[Command Group](https://docs.rs/command-group)** augments the std and tokio `Command` with
  support for process groups, portable between Unix and Windows.

- **[Event types](https://docs.rs/watchexec-events)** contains the event types used by Watchexec,
  including the JSON format used for passing event data to child processes.

- **[Signal types](https://docs.rs/watchexec-signals)** contains the signal types used by Watchexec.

- **[Ignore files](https://docs.rs/ignore-files)** finds, parses, and interprets ignore files.

- **[Project Origins](https://docs.rs/project-origins)** finds the origin (or root) path of a
  project, and what kind of project it is.

## Rust version (MSRV)

Due to the unpredictability of dependencies changing their MSRV, this library no longer tries to
keep to a minimum supported Rust version behind stable. Instead, it is assumed that developers use
the latest stable at all times.

Applications that wish to support lower-than-stable Rust (such as the Watchexec CLI does) should:
- use a lock file
- recommend the use of `--locked` when installing from source
- provide pre-built binaries (and [Binstall](https://github.com/cargo-bins/cargo-binstall) support) for non-distro users
- avoid using newer features until some time has passed, to let distro users catch up
- consider recommending that distro-Rust users switch to distro `rustup` where available
