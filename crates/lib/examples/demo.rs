use std::time::Duration;

use miette::{IntoDiagnostic, Result};
use watchexec::{
	action::{Action, EventSet, Outcome},
	command::Command,
	config::{InitConfig, RuntimeConfig},
	error::ReconfigError,
	event::Event,
	fs::Watcher,
	ErrorHook, Watchexec,
};
use watchexec_signals::Signal;

// Run with: `env RUST_LOG=debug cargo run --example print_out`
#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt::init();

	let mut init = InitConfig::default();
	init.on_error(|err: ErrorHook| async move {
		eprintln!("Watchexec Runtime Error: {}", err.error);
		Ok::<(), std::convert::Infallible>(())
	});

	let mut runtime = RuntimeConfig::default();
	runtime.pathset(["src", "dontexist", "examples"]);
	let command = Command::Exec {
		prog: "date".into(),
		args: Vec::new(),
	};

	let wx = Watchexec::new(init, runtime.clone())?;
	let w = wx.clone();

	let config = runtime.clone();
	runtime.on_action(move |action: Action| {
		let mut config = config.clone();
		let w = w.clone();
		let command = command.clone();
		async move {
			eprintln!("Watchexec Action: {action:?}");

			let sigs = action
				.events
				.iter()
				.flat_map(Event::signals)
				.collect::<Vec<_>>();

			if action.list().is_empty() {
				_ = action.create(vec![command.clone()], EventSet::All).await;
			}

			if sigs.iter().any(|sig| sig == &Signal::Interrupt) {
				for &sup in action.list() {
					action.apply(Outcome::Exit, sup, EventSet::All).await;
				}
			} else if sigs.iter().any(|sig| sig == &Signal::User1) {
				eprintln!("Switching to native for funsies");
				config.file_watcher(Watcher::Native);
				w.reconfigure(config)?;
			} else if sigs.iter().any(|sig| sig == &Signal::User2) {
				eprintln!("Switching to polling for funsies");
				config.file_watcher(Watcher::Poll(Duration::from_millis(50)));
				w.reconfigure(config)?;
			} else if action.events.iter().flat_map(Event::paths).next().is_some() {
				// TODO(Felix) Is having this pattern (a for loop over every 'alive' supervisor on
				// action creation) one you find appropriate, or would you prefer a different
				// patter?
				for &sup in action.list() {
					action
						.apply(
							Outcome::if_running(
								Outcome::both(Outcome::Stop, Outcome::Start),
								Outcome::Start,
							),
							sup,
							EventSet::All,
						)
						.await;
				}
			}

			Ok::<(), ReconfigError>(())
		}
	});

	wx.reconfigure(runtime)?;
	wx.main().await.into_diagnostic()??;

	Ok(())
}
