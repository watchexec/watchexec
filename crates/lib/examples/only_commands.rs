use std::time::{Duration, Instant};

use miette::{IntoDiagnostic, Result};
use tokio::time::sleep;
use watchexec::{
	action::Action,
	command::{Command, Program},
	Watchexec,
};
use watchexec_events::{Event, Priority};

#[tokio::main]
async fn main() -> Result<()> {
	let wx = Watchexec::new(|mut action: Action| {
		// you don't HAVE to respond to filesystem events:
		// here, we start a command every five seconds, unless we get a signal and quit

		if action.signals().next().is_some() {
			eprintln!("[Quitting...]");
			action.quit();
		} else {
			let (_, job) = action.create_job(Command {
				program: Program::Exec {
					prog: "echo".into(),
					args: vec![
						"Hello world!".into(),
						format!("Current time: {:?}", Instant::now()),
						"Press Ctrl+C to quit".into(),
					],
				},
				options: Default::default(),
			});
			job.start();
		}

		action
	})?;

	tokio::spawn({
		let wx = wx.clone();
		async move {
			loop {
				sleep(Duration::from_secs(5)).await;
				wx.send_event(Event::default(), Priority::Urgent)
					.await
					.unwrap();
			}
		}
	});

	let _ = wx.main().await.into_diagnostic()?;
	Ok(())
}
