use std::time::Duration;

use tokio::time::sleep;
use watchexec::{
	config::{InitConfig, RuntimeConfig},
	Watchexec,
};

#[tokio::main]
async fn main() -> color_eyre::eyre::Result<()> {
	tracing_subscriber::fmt::init();
	color_eyre::install()?;

	let mut init = InitConfig::builder();
	init.on_error(|err| async move {
		eprintln!("Watchexec Runtime Error: {}", err);
		Ok::<(), std::convert::Infallible>(())
	});

	let runtime = RuntimeConfig::default();

	let wx = Watchexec::new(init.build()?, runtime)?;
	wx.main();

	// TODO: induce an error here

	sleep(Duration::from_secs(1)).await;

	Ok(())
}
