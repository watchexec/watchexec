#![deny(rust_2018_idioms)]

use std::{env::var, fs::File, sync::Mutex};

use miette::{IntoDiagnostic, Result};
use tracing::debug;
use tracing_log::LogTracer;
use watchexec::{
	event::{Event, Priority},
	Watchexec,
};

mod args;
mod config;
mod filterer;

#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> Result<()> {
	LogTracer::init().into_diagnostic()?;

	#[cfg(feature = "dev-console")]
	console_subscriber::init();

	if var("RUST_LOG").is_ok() && cfg!(not(feature = "dev-console")) {
		tracing_subscriber::fmt::init();
	}

	let tagged_filterer = var("WATCHEXEC_FILTERER")
		.map(|v| v == "tagged")
		.unwrap_or(false);

	let args = args::get_args(tagged_filterer)?;

	{
		let verbosity = args.occurrences_of("verbose");
		let log_file = if let Some(file) = args.value_of("log-file") {
			Some(File::create(file).into_diagnostic()?)
		} else {
			None
		};

		let mut builder = tracing_subscriber::fmt().with_env_filter(match verbosity {
			0 => "watchexec-cli=warn",
			1 => "watchexec=debug,watchexec-filterer-globset=debug,watchexec-filterer-ignore=debug,watchexec-filterer-tagged=debug,watchexec-cli=debug",
			2 => "ignore-files=trace,project-origins=trace,watchexec=trace,watchexec-filterer-globset=trace,watchexec-filterer-ignore=trace,watchexec-filterer-tagged=trace,watchexec-cli=trace",
			_ => "trace",
		});

		if verbosity > 2 {
			use tracing_subscriber::fmt::format::FmtSpan;
			builder = builder.with_span_events(FmtSpan::NEW | FmtSpan::CLOSE);
		}

		if let Some(writer) = log_file {
			builder
				.json()
				.with_writer(Mutex::new(writer))
				.try_init()
				.ok();
		} else if verbosity > 3 {
			builder.pretty().try_init().ok();
		} else {
			builder.try_init().ok();
		}
	}

	debug!(version=%env!("CARGO_PKG_VERSION"), ?args, "constructing Watchexec from CLI");

	let init = config::init(&args)?;
	let mut runtime = config::runtime(&args)?;
	runtime.filterer(if tagged_filterer {
		eprintln!("!!! EXPERIMENTAL: using tagged filterer !!!");
		filterer::tagged(&args).await?
	} else {
		filterer::globset(&args).await?
	});

	let wx = Watchexec::new(init, runtime)?;

	if !args.is_present("postpone") {
		wx.send_event(Event::default(), Priority::Urgent).await?;
	}

	wx.main().await.into_diagnostic()??;

	Ok(())
}
