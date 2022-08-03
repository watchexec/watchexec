#![deny(rust_2018_idioms)]

use std::{env::var, fs::File, sync::Mutex};

use miette::{IntoDiagnostic, Result};
use tracing::{info, warn, debug};
use watchexec::{
	event::{Event, Priority},
	Watchexec,
};

mod args;
mod config;
mod filterer;

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
	let verbosity = args.occurrences_of("verbose");

	if log_on {
		warn!("ignoring logging options from args");
	} else if verbosity > 0 {
		let log_file = if let Some(file) = args.value_of_os("log-file") {
			// TODO: use tracing-appender instead
			Some(File::create(file).into_diagnostic()?)
		} else {
			None
		};

		let mut builder = tracing_subscriber::fmt().with_env_filter(match verbosity {
			0 => unreachable!("checked by if earlier"),
			1 => "warn",
			2 => "info",
			3 => "debug",
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

	info!(version=%env!("CARGO_PKG_VERSION"), "constructing Watchexec from CLI");
	debug!(?args, "arguments");

	let init = config::init(&args)?;
	let mut runtime = config::runtime(&args)?;
	runtime.filterer(if tagged_filterer {
		eprintln!("!!! EXPERIMENTAL: using tagged filterer !!!");
		filterer::tagged(&args).await?
	} else {
		filterer::globset(&args).await?
	});

	info!("initialising Watchexec runtime");
	let wx = Watchexec::new(init, runtime)?;

	if !args.is_present("postpone") {
		debug!("kicking off with empty event");
		wx.send_event(Event::default(), Priority::Urgent).await?;
	}

	info!("running main loop");
	wx.main().await.into_diagnostic()??;
	info!("done with main loop");

	Ok(())
}
