use miette::{IntoDiagnostic, Result};
use watchexec::{action::Action, Watchexec};

#[tokio::main]
async fn main() -> Result<()> {
	let wx = Watchexec::new(|mut action: Action| {
		// you don't HAVE to spawn jobs:
		// here, we just print out the events as they come in
		for event in action.events.iter() {
			eprintln!("{event:?}");
		}

		// quit when we get a signal
		if action.signals().next().is_some() {
			eprintln!("[Quitting...]");
			action.quit();
		}

		action
	})?;

	// start the engine
	let main = wx.main();

	// and watch all files in the current directory:
	wx.config.pathset(["."]);

	let _ = main.await.into_diagnostic()?;
	Ok(())
}
