#![deny(rust_2018_idioms)]
#![allow(clippy::missing_const_for_fn, clippy::future_not_send)]

use std::{env::var, fs::File, io::Write, process::Stdio, sync::Mutex};

use args::{Args, ShellCompletion};
use clap::CommandFactory;
use clap_complete::{Generator, Shell};
use clap_mangen::Man;
use is_terminal::IsTerminal;
use miette::{IntoDiagnostic, Result};
use tokio::{fs::metadata, io::AsyncWriteExt, process::Command};
use tracing::{debug, info, warn};
use watchexec::Watchexec;
use watchexec_events::{Event, Priority};

use crate::filterer::WatchexecFilterer;

pub mod args;
mod config;
mod emits;
mod filterer;
mod state;

async fn init() -> Result<Args> {
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

	let args = args::get_args().await?;
	let verbosity = args.verbose.unwrap_or(0);

	if log_on {
		warn!("ignoring logging options from args");
	} else if verbosity > 0 {
		let log_file = if let Some(file) = &args.log_file {
			let is_dir = metadata(&file).await.map_or(false, |info| info.is_dir());
			let path = if is_dir {
				let filename = format!(
					"watchexec.{}.log",
					chrono::Utc::now().format("%Y-%m-%dT%H-%M-%SZ")
				);
				file.join(filename)
			} else {
				file.to_owned()
			};

			// TODO: use tracing-appender instead
			Some(File::create(path).into_diagnostic()?)
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
			Ok(()) => info!("logging initialised"),
			Err(e) => eprintln!("Failed to initialise logging, continuing with none\n{e}"),
		}
	}

	Ok(args)
}

async fn run_watchexec(args: Args) -> Result<()> {
	info!(version=%env!("CARGO_PKG_VERSION"), "constructing Watchexec from CLI");

	let state = state::State::default();
	let config = config::make_config(&args, &state)?;
	config.filterer(WatchexecFilterer::new(&args).await?);

	info!("initialising Watchexec runtime");
	let wx = Watchexec::with_config(config)?;

	if !args.postpone {
		debug!("kicking off with empty event");
		wx.send_event(Event::default(), Priority::Urgent).await?;
	}

	info!("running main loop");
	wx.main().await.into_diagnostic()??;

	if matches!(args.screen_clear, Some(args::ClearMode::Reset)) {
		config::reset_screen();
	}

	info!("done with main loop");

	Ok(())
}

async fn run_manpage(_args: Args) -> Result<()> {
	info!(version=%env!("CARGO_PKG_VERSION"), "constructing manpage");

	let man = Man::new(Args::command().long_version(None));
	let mut buffer: Vec<u8> = Default::default();
	man.render(&mut buffer).into_diagnostic()?;

	if std::io::stdout().is_terminal() && which::which("man").is_ok() {
		let mut child = Command::new("man")
			.arg("-l")
			.arg("-")
			.stdin(Stdio::piped())
			.stdout(Stdio::inherit())
			.stderr(Stdio::inherit())
			.kill_on_drop(true)
			.spawn()
			.into_diagnostic()?;
		child
			.stdin
			.as_mut()
			.unwrap()
			.write_all(&buffer)
			.await
			.into_diagnostic()?;

		if let Some(code) = child
			.wait()
			.await
			.into_diagnostic()?
			.code()
			.and_then(|code| if code == 0 { None } else { Some(code) })
		{
			return Err(miette::miette!("Exited with status code {}", code));
		}
	} else {
		std::io::stdout()
			.lock()
			.write_all(&buffer)
			.into_diagnostic()?;
	}

	Ok(())
}

#[allow(clippy::unused_async)]
async fn run_completions(shell: ShellCompletion) -> Result<()> {
	fn generate(generator: impl Generator) {
		let mut cmd = Args::command();
		clap_complete::generate(generator, &mut cmd, "watchexec", &mut std::io::stdout());
	}

	info!(version=%env!("CARGO_PKG_VERSION"), "constructing completions");

	match shell {
		ShellCompletion::Bash => generate(Shell::Bash),
		ShellCompletion::Elvish => generate(Shell::Elvish),
		ShellCompletion::Fish => generate(Shell::Fish),
		ShellCompletion::Nu => generate(clap_complete_nushell::Nushell),
		ShellCompletion::Powershell => generate(Shell::PowerShell),
		ShellCompletion::Zsh => generate(Shell::Zsh),
	}

	Ok(())
}

pub async fn run() -> Result<()> {
	let args = init().await?;
	debug!(?args, "arguments");

	if args.manual {
		run_manpage(args).await
	} else if let Some(shell) = args.completions {
		run_completions(shell).await
	} else {
		run_watchexec(args).await
	}
}
