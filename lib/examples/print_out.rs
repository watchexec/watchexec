use std::time::Duration;

use tokio::time::sleep;
use watchexec::{
	config::{InitConfigBuilder, RuntimeConfig},
	Watchexec,
};

// Run with: `env RUST_LOG=debug cargo run --example print_out`
#[tokio::main]
async fn main() -> color_eyre::eyre::Result<()> {
	tracing_subscriber::fmt::init();
	color_eyre::install()?;

	let mut init = InitConfigBuilder::default();
	init.on_error(|err| async move {
		eprintln!("Watchexec Runtime Error: {}", err);
		Ok::<(), std::convert::Infallible>(())
	});

	let runtime = RuntimeConfig::default();

	let wx = Watchexec::new(init.build()?, runtime)?;
	wx.main();

	sleep(Duration::from_secs(1)).await;

	Ok(())
}
