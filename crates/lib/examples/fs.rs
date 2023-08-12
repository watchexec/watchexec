use std::time::Duration;

use async_priority_channel as priority;
use miette::{IntoDiagnostic, Result};
use tokio::{
	sync::{mpsc, watch},
	time::sleep,
};
use watchexec::fs;
use watchexec_events::{Event, Priority};

// Run with: `env RUST_LOG=debug cargo run --example fs`,
// then touch some files within the first 15 seconds, and afterwards.
#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt::init();

	let (ev_s, ev_r) = priority::bounded::<Event, Priority>(1024);
	let (er_s, mut er_r) = mpsc::channel(64);
	let (wd_s, wd_r) = watch::channel(fs::WorkingData::default());

	let mut wkd = fs::WorkingData::default();
	wkd.pathset = vec![".".into()];
	wd_s.send(wkd.clone()).into_diagnostic()?;

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

	let wd_sh = tokio::spawn(async move {
		sleep(Duration::from_secs(15)).await;
		wkd.pathset = Vec::new();
		tracing::info!("turning off fs watcher without stopping it");
		wd_s.send(wkd).unwrap();
		wd_s
	});

	fs::worker(wd_r, er_s, ev_s).await?;
	wd_sh.await.into_diagnostic()?;

	Ok(())
}
