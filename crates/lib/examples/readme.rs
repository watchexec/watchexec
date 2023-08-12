use miette::{IntoDiagnostic, Result};
use std::convert::Infallible;
use watchexec::{
	action::{Action, EventSet, Outcome},
	command::{Program, Shell},
	config::{InitConfig, RuntimeConfig},
	handler::{sync, PrintDebug},
	Watchexec,
};
use watchexec_events::{Event, Priority};
use watchexec_signals::Signal;

#[tokio::main]
async fn main() -> Result<()> {
	let mut init = InitConfig::default();
	init.on_error(PrintDebug(std::io::stderr()));
	// ^ this is okay to start with but quickly gets much too verbose,
	//   substitute your own error handling appropriate for your app!

	// define a simple initial configuration
	let mut runtime = RuntimeConfig::default();
	runtime.on_action(sync(|action: Action| -> Result<(), Infallible> {
		let id = action.create(
			Program::Shell {
				shell: Shell::new("bash"),
				command: "
				echo 'Hello world';
				trap INT 'echo Not quitting yet!';
				read
			"
				.into(),
				args: Vec::new(),
			}
			.into(),
		);
		action.apply(id, Outcome::Start, EventSet::All);
		Ok(())
	}));

	// Initialise Watchexec
	let we = Watchexec::new(init, runtime.clone())?;
	// start the engine
	let main = we.main();

	// send an event to start
	we.send_event(Event::default(), Priority::Urgent)
		.await
		.unwrap();
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
