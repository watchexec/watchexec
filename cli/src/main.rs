use std::{env::var};

use miette::{IntoDiagnostic, Result};
use watchexec::{Watchexec, event::Event, filter::tagged::{Filter, Matcher, Op, TaggedFilterer}};

mod args;
mod config;

#[tokio::main]
async fn main() -> Result<()> {
	#[cfg(feature = "dev-console")]
	console_subscriber::init();

	if var("RUST_LOG").is_ok() && cfg!(not(feature = "dev-console")) {
		tracing_subscriber::fmt::init();
	}

	let args = args::get_args()?;

	if args.is_present("verbose") {
		tracing_subscriber::fmt()
			.with_env_filter(match args.occurrences_of("verbose") {
				0 => unreachable!(),
				1 => "watchexec=debug",
				2 => "watchexec=trace",
				_ => "trace",
			})
			.try_init()
			.ok();
	}

	let mut filters = Vec::new();

	// TODO: move into config?
	for filter in args.values_of("filter").unwrap_or_default() {
		filters.push(filter.parse()?);
	}

	for ext in args.values_of("extensions").unwrap_or_default().map(|s| s.split(',').map(|s| s.trim())).flatten() {
		filters.push(Filter {
			in_path: None,
			on: Matcher::Path,
			op: Op::Glob,
			pat: TaggedFilterer::glob(&format!("**/*.{}", ext))?,
			negate: false,
		});
	}

	let (init, runtime, filterer) = config::new(&args)?;
	filterer.add_filters(&filters).await?;

	let wx = Watchexec::new(init, runtime)?;

	if !args.is_present("postpone") {
		wx.send_event(Event::default()).await?;
	}

	wx.main().await.into_diagnostic()??;

	Ok(())
}
