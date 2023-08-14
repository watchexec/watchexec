use std::{sync::Arc, time::Duration};

use async_priority_channel as priority;
use miette::{IntoDiagnostic, Result};
use tokio::{sync::mpsc, time::sleep};
use watchexec::{fs, Config};
use watchexec_events::{Event, Priority};

// Run with: `env RUST_LOG=debug cargo run --example fs`,
// then touch some files within the first 15 seconds, and afterwards.
#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt::init();

	let (ev_s, ev_r) = priority::bounded::<Event, Priority>(1024);
	let (er_s, mut er_r) = mpsc::channel(64);

	let config = Arc::new(Config::default());
	config.pathset(["."]);

	tokio::spawn(async move {
		while let Ok((event, priority)) = ev_r.recv().await {
			tracing::info!("event ({priority:?}): {event:?}");
		}
	});

	tokio::spawn(async move {
		while let Some(error) = er_r.recv().await {
			tracing::error!("error: {error}");
		}
	});

	let shutdown = tokio::spawn({
		let config = config.clone();
		async move {
			sleep(Duration::from_secs(15)).await;
			tracing::info!("turning off fs watcher without stopping it");
			config.pathset(Vec::<String>::new());
		}
	});

	fs::worker(config, er_s, ev_s).await?;
	shutdown.await.into_diagnostic()?;

	Ok(())
}
