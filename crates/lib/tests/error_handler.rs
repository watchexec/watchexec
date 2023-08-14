use std::time::Duration;

use miette::Result;
use tokio::time::sleep;
use watchexec::{ErrorHook, Watchexec};

#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt::init();

	let wx = Watchexec::default();
	wx.config.on_error(|err: ErrorHook| {
		eprintln!("Watchexec Runtime Error: {}", err.error);
	});
	wx.main();

	// TODO: induce an error here

	sleep(Duration::from_secs(1)).await;

	Ok(())
}
