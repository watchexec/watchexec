use std::time::Duration;

use tokio::{
	sync::{mpsc, watch},
	time::sleep,
};
use watchexec::{event::Event, fs};

// Run with: `env RUST_LOG=debug cargo run --example fs`,
// then touch some files within the first 15 seconds, and afterwards.
#[tokio::main]
async fn main() -> color_eyre::eyre::Result<()> {
	tracing_subscriber::fmt::init();
	color_eyre::install()?;

	let (ev_s, mut ev_r) = mpsc::channel::<Event>(1024);
	let (er_s, mut er_r) = mpsc::channel(64);
	let (wd_s, wd_r) = watch::channel(fs::WorkingData::default());

	let mut wkd = fs::WorkingData::default();
	wkd.pathset = vec![".".into()];
	wd_s.send(wkd.clone())?;

	tokio::spawn(async move {
		while let Some(e) = ev_r.recv().await {
			tracing::info!("event: {:?}", e);
		}
	});

	tokio::spawn(async move {
		while let Some(e) = er_r.recv().await {
			tracing::error!("error: {}", e);
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
	wd_sh.await?;

	Ok(())
}
