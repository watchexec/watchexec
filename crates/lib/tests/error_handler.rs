use std::time::Duration;

use miette::Result;
use tokio::time::sleep;
use watchexec::{Config, ErrorHook, Watchexec};

#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt::init();

	let mut config = Config::default();
	config.on_error(|err: ErrorHook| {
		eprintln!("Watchexec Runtime Error: {}", err.error);
	});

	let wx = Watchexec::new(config)?;
	wx.main();

	// TODO: induce an error here

	sleep(Duration::from_secs(1)).await;

	Ok(())
}
