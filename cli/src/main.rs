use std::env::var;

use color_eyre::eyre::Result;
use tracing_subscriber::filter::LevelFilter;
use watchexec::Watchexec;

mod args;
mod config;

#[tokio::main]
async fn main() -> Result<()> {
	color_eyre::install()?;

	if var("RUST_LOG").is_ok() {
		tracing_subscriber::fmt::init();
	}

	let args = args::get_args()?;

	if args.is_present("verbose") {
		tracing_subscriber::fmt()
			.with_max_level(LevelFilter::DEBUG)
			.try_init()
			.ok();
	}

	let (init, runtime) = config::new(&args)?;

	let config = runtime.clone();
	let wx = Watchexec::new(init, runtime)?;

	wx.main().await??;

	Ok(())
}
