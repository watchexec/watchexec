use std::error::Error;

use tokio::sync::{mpsc, watch};
use watchexec::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
	tracing_subscriber::fmt::init();

	let (ev_s, mut ev_r) = mpsc::channel(1024);
	let (er_s, mut er_r) = mpsc::channel(64);
	let (wd_s, wd_r) = watch::channel(fs::WorkingData::default());

	let mut wkd = fs::WorkingData::default();
	wkd.pathset = vec![".".into()];
	wd_s.send(wkd)?;

	tokio::spawn(async move {
		while let Some(e) = ev_r.recv().await {
			println!("event: {:?}", e);
		}
	});

	tokio::spawn(async move {
		while let Some(e) = er_r.recv().await {
			println!("error: {}", e);
		}
	});

	fs::worker(wd_r, er_s, ev_s).await?;

	Ok(())
}
