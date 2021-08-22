use std::convert::Infallible;

use watchexec::{
	action::{Action, Outcome},
	config::{InitConfigBuilder, RuntimeConfig},
	signal::Signal,
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

	let mut runtime = RuntimeConfig::default();
	runtime.command(["date"]);
	runtime.on_action(|action: Action| async move {
		eprintln!("Watchexec Action: {:?}", action);

		if action
			.events
			.iter()
			.flat_map(|event| event.signals())
			.any(|sig| sig == Signal::Interrupt)
		{
			action.outcome(Outcome::Exit);
		} else {
			action.outcome(Outcome::both(Outcome::Stop, Outcome::Start));
		}

		Ok::<(), Infallible>(())
	});

	let wx = Watchexec::new(init.build()?, runtime)?;
	wx.main().await??;

	Ok(())
}
