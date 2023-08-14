use miette::{IntoDiagnostic, Result};
use watchexec::{
	action::{Action, EventSet, Outcome},
	command::{Program, Shell},
	Watchexec,
};
use watchexec_events::{Event, Priority};
use watchexec_signals::Signal;

#[tokio::main]
async fn main() -> Result<()> {
	// this is okay to start with, but Watchexec logs a LOT of data,
	// even at error level. you will quickly want to filter it down.
	tracing_subscriber::fmt::init();

	// initialise Watchexec with a simple initial action handler
	let wx = Watchexec::new(|action: Action| {
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
	})?;
	// start the engine
	let main = wx.main();

	// send an event to start
	wx.send_event(Event::default(), Priority::Urgent)
		.await
		.unwrap();
	// ^ this will cause the action handler we've defined above to run,
	//   creating and starting our little bash program

	// now we change what the action does:
	wx.config.on_action(|action: Action| {
		// if we get Ctrl-C on the Watchexec instance, we quit
		if action.signals().any(|sig| sig == Signal::Interrupt) {
			action.quit();
			return;
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
	});

	// and watch all files in the current directory:
	wx.config.pathset(["."]);

	// then keep running until Watchexec quits!
	let _ = main.await.into_diagnostic()?;
	Ok(())
}
