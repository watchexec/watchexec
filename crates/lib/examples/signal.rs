use std::process::exit;

use async_priority_channel as priority;
use miette::Result;
use tokio::sync::mpsc;
use watchexec::{
	event::{Event, Priority, Tag},
	signal,
};
use watchexec_signals::Signal;

// Run with: `env RUST_LOG=debug cargo run --example signal`,
// then issue some signals to the printed PID, or hit e.g. Ctrl-C.
// Send a SIGTERM (unix) or Ctrl-Break (windows) to exit.
#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt::init();

	let (ev_s, ev_r) = priority::bounded::<Event, Priority>(1024);
	let (er_s, mut er_r) = mpsc::channel(64);

	tokio::spawn(async move {
		while let Ok((event, priority)) = ev_r.recv().await {
			tracing::info!("event {priority:?}: {event:?}");

			if event.tags.contains(&Tag::Signal(Signal::Terminate)) {
				exit(0);
			}
		}
	});

	tokio::spawn(async move {
		while let Some(error) = er_r.recv().await {
			tracing::error!("error: {error}");
		}
	});

	tracing::info!("PID is {}", std::process::id());
	signal::worker(er_s.clone(), ev_s.clone()).await?;

	Ok(())
}
