use std::process::exit;

use miette::Result;
use tokio::sync::mpsc;
use watchexec::{
	event::{Event, Tag},
	signal::{self, source::MainSignal},
};

// Run with: `env RUST_LOG=debug cargo run --example signal`,
// then issue some signals to the printed PID, or hit e.g. Ctrl-C.
// Send a SIGTERM (unix) or Ctrl-Break (windows) to exit.
#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt::init();

	let (ev_s, mut ev_r) = mpsc::channel::<Event>(1024);
	let (er_s, mut er_r) = mpsc::channel(64);

	tokio::spawn(async move {
		while let Some(e) = ev_r.recv().await {
			tracing::info!("event: {:?}", e);

			if e.tags.contains(&Tag::Signal(MainSignal::Terminate)) {
				exit(0);
			}
		}
	});

	tokio::spawn(async move {
		while let Some(e) = er_r.recv().await {
			tracing::error!("{}", e);
		}
	});

	tracing::info!("PID is {}", std::process::id());
	signal::source::worker(er_s.clone(), ev_s.clone()).await?;

	Ok(())
}
