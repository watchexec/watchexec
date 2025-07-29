#![deny(rust_2018_idioms)]
#![allow(clippy::missing_const_for_fn, clippy::future_not_send)]

use std::{
	io::{IsTerminal, Write},
	process::{ExitCode, Stdio},
};

use clap::CommandFactory;
use clap_complete::{Generator, Shell};
use clap_mangen::Man;
use miette::{IntoDiagnostic, Result};
use tokio::{io::AsyncWriteExt, process::Command};
use tracing::{debug, info};
use watchexec::Watchexec;
use watchexec_events::{Event, Priority};

use crate::{
	args::{Args, ShellCompletion},
	filterer::WatchexecFilterer,
};

pub mod args;
mod config;
mod dirs;
mod emits;
mod filterer;
mod socket;
mod state;

async fn run_watchexec(args: Args, state: state::State) -> Result<()> {
	info!(version=%env!("CARGO_PKG_VERSION"), "constructing Watchexec from CLI");

	let config = config::make_config(&args, &state)?;
	config.filterer(WatchexecFilterer::new(&args).await?);

	let wx = Watchexec::with_config(config)?;

	if !args.events.postpone {
		debug!("kicking off with empty event");
		wx.send_event(Event::default(), Priority::Urgent).await?;
	}

	info!("running main loop");
	wx.main().await.into_diagnostic()??;

	if matches!(
		args.output.screen_clear,
		Some(args::output::ClearMode::Reset)
	) {
		config::reset_screen();
	}

	info!("done with main loop");

	Ok(())
}

async fn run_manpage() -> Result<()> {
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

pub async fn run() -> Result<ExitCode> {
	let (args, _guards) = args::parse_args().await?;

	Ok(if args.manual {
		run_manpage().await?;
		ExitCode::SUCCESS
	} else if let Some(shell) = args.completions {
		run_completions(shell).await?;
		ExitCode::SUCCESS
	} else {
		let state = state::new(&args).await?;
		run_watchexec(args, state.clone()).await?;
		let exit = *(state.exit_code.lock().unwrap());
		exit
	})
}
