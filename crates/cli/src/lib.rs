#![deny(rust_2018_idioms)]

use std::{env::var, fs::File, sync::Mutex};

use miette::{IntoDiagnostic, Result};
use tracing::{debug, info, warn};
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

pub async fn run() -> Result<()> {
	let mut log_on = false;

	#[cfg(feature = "dev-console")]
	match console_subscriber::try_init() {
		Ok(_) => {
			warn!("dev-console enabled");
			log_on = true;
		}
		Err(e) => {
			eprintln!("Failed to initialise tokio console, falling back to normal logging\n{e}")
		}
	}

	if !log_on && var("RUST_LOG").is_ok() {
		match tracing_subscriber::fmt::try_init() {
			Ok(_) => {
				warn!(RUST_LOG=%var("RUST_LOG").unwrap(), "logging configured from RUST_LOG");
				log_on = true;
			}
			Err(e) => eprintln!("Failed to initialise logging with RUST_LOG, falling back\n{e}"),
		}
	}

	let tagged_filterer = var("WATCHEXEC_FILTERER")
		.map(|v| v == "tagged")
		.unwrap_or(false);

	let args = args::get_args(tagged_filterer)?;

	if log_on {
		warn!("ignoring logging options from args");
	} else {
		let verbosity = args.occurrences_of("verbose");
		let log_file = if let Some(file) = args.value_of_os("log-file") {
			// TODO: use tracing-appender instead
			Some(File::create(file).into_diagnostic()?)
		} else {
			None
		};

		let mut builder = tracing_subscriber::fmt().with_env_filter(match verbosity {
			0 => "watchexec_cli=warn",
			1 => "watchexec=debug,watchexec_filterer_globset=debug,watchexec_filterer_ignore=debug,watchexec_filterer_tagged=debug,watchexec_cli=debug",
			2 => "ignore_files=trace,project_origins=trace,watchexec=trace,watchexec_filterer_globset=trace,watchexec_filterer_ignore=trace,watchexec_filterer_tagged=trace,watchexec_cli=trace",
			_ => "trace",
		});

		if verbosity > 2 {
			use tracing_subscriber::fmt::format::FmtSpan;
			builder = builder.with_span_events(FmtSpan::NEW | FmtSpan::CLOSE);
		}

		match if let Some(writer) = log_file {
			builder.json().with_writer(Mutex::new(writer)).try_init()
		} else if verbosity > 3 {
			builder.pretty().try_init()
		} else {
			builder.try_init()
		} {
			Ok(_) => info!("logging initialised"),
			Err(e) => eprintln!("Failed to initialise logging, continuing with none\n{e}"),
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
