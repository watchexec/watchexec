use std::sync::Arc;

use miette::{IntoDiagnostic, Result};
use watchexec::{
	command::{Command, Program, SpawnOptions},
	job::CommandState,
	Id, Watchexec,
};
use watchexec_events::{Event, Priority, ProcessEnd};
use watchexec_signals::Signal;

#[tokio::main]
async fn main() -> Result<()> {
	let build_id = Id::default();
	let run_id = Id::default();
	let wx = Watchexec::new_async(move |mut action| {
		Box::new(async move {
			if action.signals().any(|sig| sig == Signal::Interrupt) {
				eprintln!("[Quitting...]");
				action.quit();
				return action;
			}

			let build = action.get_or_create_job(build_id, || {
				Arc::new(Command {
					program: Program::Exec {
						prog: "cargo".into(),
						args: vec!["build".into()],
					},
					options: Default::default(),
				})
			});

			let run = action.get_or_create_job(run_id, || {
				Arc::new(Command {
					program: Program::Exec {
						prog: "cargo".into(),
						args: vec!["run".into()],
					},
					options: SpawnOptions {
						grouped: true,
						..Default::default()
					},
				})
			});

			if action.paths().next().is_some()
				|| action.events.iter().any(|event| event.tags.is_empty())
			{
				build.restart().await;
			}

			build.to_wait().await;
			build
				.run(move |context| {
					if let CommandState::Finished {
						status: ProcessEnd::Success,
						..
					} = context.current
					{
						run.restart();
					}
				})
				.await;

			action
		})
	})?;

	// start the engine
	let main = wx.main();

	// send an event to start
	wx.send_event(Event::default(), Priority::Urgent)
		.await
		.unwrap();

	// and watch all files in cli src
	wx.config.pathset(["crates/cli/src"]);

	// then keep running until Watchexec quits!
	let _ = main.await.into_diagnostic()?;
	Ok(())
}
