use std::{env::var, io::stderr, path::PathBuf};

use clap::{ArgAction, Parser, ValueHint};
use miette::{bail, Result};
use tokio::fs::metadata;
use tracing::{info, warn};
use tracing_appender::{non_blocking, non_blocking::WorkerGuard, rolling};

#[derive(Debug, Clone, Parser)]
pub struct LoggingArgs {
	/// Set diagnostic log level
	///
	/// This enables diagnostic logging, which is useful for investigating bugs or gaining more
	/// insight into faulty filters or "missing" events. Use multiple times to increase verbosity.
	///
	/// Goes up to '-vvvv'. When submitting bug reports, default to a '-vvv' log level.
	///
	/// You may want to use with '--log-file' to avoid polluting your terminal.
	///
	/// Setting $RUST_LOG also works, and takes precedence, but is not recommended. However, using
	/// $RUST_LOG is the only way to get logs from before these options are parsed.
	#[arg(
		long,
		short,
		help_heading = super::OPTSET_DEBUGGING,
		action = ArgAction::Count,
		default_value = "0",
		num_args = 0,
	)]
	pub verbose: u8,

	/// Write diagnostic logs to a file
	///
	/// This writes diagnostic logs to a file, instead of the terminal, in JSON format. If a log
	/// level was not already specified, this will set it to '-vvv'.
	///
	/// If a path is not provided, the default is the working directory. Note that with
	/// '--ignore-nothing', the write events to the log will likely get picked up by Watchexec,
	/// causing a loop; prefer setting a path outside of the watched directory.
	///
	/// If the path provided is a directory, a file will be created in that directory. The file name
	/// will be the current date and time, in the format 'watchexec.YYYY-MM-DDTHH-MM-SSZ.log'.
	#[arg(
		long,
		help_heading = super::OPTSET_DEBUGGING,
		num_args = 0..=1,
		default_missing_value = ".",
		value_hint = ValueHint::AnyPath,
		value_name = "PATH",
	)]
	pub log_file: Option<PathBuf>,
}

pub fn preargs() -> bool {
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
			Ok(()) => {
				warn!(RUST_LOG=%var("RUST_LOG").unwrap(), "logging configured from RUST_LOG");
				log_on = true;
			}
			Err(e) => eprintln!("Failed to initialise logging with RUST_LOG, falling back\n{e}"),
		}
	}

	log_on
}

pub async fn postargs(args: &LoggingArgs) -> Result<Option<WorkerGuard>> {
	if args.verbose == 0 {
		return Ok(None);
	}

	let (log_writer, guard) = if let Some(file) = &args.log_file {
		let is_dir = metadata(&file).await.map_or(false, |info| info.is_dir());
		let (dir, filename) = if is_dir {
			(
				file.to_owned(),
				PathBuf::from(format!(
					"watchexec.{}.log",
					chrono::Utc::now().format("%Y-%m-%dT%H-%M-%SZ")
				)),
			)
		} else if let (Some(parent), Some(file_name)) = (file.parent(), file.file_name()) {
			(parent.into(), PathBuf::from(file_name))
		} else {
			bail!("Failed to determine log file name");
		};

		non_blocking(rolling::never(dir, filename))
	} else {
		non_blocking(stderr())
	};

	let mut builder = tracing_subscriber::fmt().with_env_filter(match args.verbose {
		0 => unreachable!("checked by if earlier"),
		1 => "warn",
		2 => "info",
		3 => "debug",
		_ => "trace",
	});

	if args.verbose > 2 {
		use tracing_subscriber::fmt::format::FmtSpan;
		builder = builder.with_span_events(FmtSpan::NEW | FmtSpan::CLOSE);
	}

	match if args.log_file.is_some() {
		builder.json().with_writer(log_writer).try_init()
	} else if args.verbose > 3 {
		builder.pretty().with_writer(log_writer).try_init()
	} else {
		builder.with_writer(log_writer).try_init()
	} {
		Ok(()) => info!("logging initialised"),
		Err(e) => eprintln!("Failed to initialise logging, continuing with none\n{e}"),
	}

	Ok(Some(guard))
}
