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
use std::sync::Arc;
use tokio::{io::AsyncWriteExt, process::Command, sync::watch, task::JoinHandle};
use tracing::{debug, info};
use watchexec::{error::CriticalError, Watchexec};
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
	let initial_ready = args.events.initial_events.map(|_| config.fs_ready());

	info!("initialising Watchexec runtime");
	let wx = Arc::new(Watchexec::with_config(config)?);

	// Set the watchexec reference in state so it can be used for sending synthetic events
	state
		.watchexec
		.set(wx.clone())
		.expect("watchexec reference already set");

	if !args.events.postpone && initial_ready.is_none() {
		debug!("kicking off with empty event");
		wx.send_event(Event::default(), Priority::Urgent).await?;
	}

	if args.events.interactive {
		eprintln!("[Interactive] q: quit, p: pause/unpause, r: restart, s: stop");
	}

	info!("running main loop");
	let mut main = wx.main();
	let runtime_ended = if let Some(ready) = initial_ready {
		if wait_for_fs_ready(ready, &mut main).await? {
			if !args.events.postpone {
				debug!("kicking off with empty event after initial paths are ready");
				wx.send_event(Event::default(), Priority::Urgent).await?;
			}
			false
		} else {
			true
		}
	} else {
		false
	};
	if !runtime_ended {
		main.await.into_diagnostic()??;
	}

	if matches!(
		args.output.screen_clear,
		Some(args::output::ClearMode::Reset)
	) {
		config::reset_screen();
	}

	info!("done with main loop");

	Ok(())
}

async fn wait_for_fs_ready(
	mut ready: watch::Receiver<()>,
	main: &mut JoinHandle<Result<(), CriticalError>>,
) -> Result<bool> {
	tokio::select! {
		biased;
		result = main => {
			result.into_diagnostic()??;
			Ok(false)
		}
		result = ready.changed() => {
			result.into_diagnostic()?;
			Ok(true)
		}
	}
}

#[cfg(test)]
mod startup_tests {
	use std::{future::pending, time::Duration};

	use super::{wait_for_fs_ready, CriticalError};
	use tokio::sync::watch;

	#[test]
	fn initial_readiness_returns_runtime_failure() {
		tokio::runtime::Builder::new_current_thread()
			.enable_time()
			.build()
			.unwrap()
			.block_on(async {
				let (_sender, ready) = watch::channel(());
				let mut main =
					tokio::spawn(async { Err(CriticalError::External("startup failed".into())) });
				let result = tokio::time::timeout(
					Duration::from_secs(1),
					wait_for_fs_ready(ready, &mut main),
				)
				.await
				.expect("runtime failure must not wait for filesystem readiness");
				assert!(result.unwrap_err().to_string().contains("startup failed"));
			});
	}

	#[test]
	fn initial_readiness_returns_runtime_cancellation() {
		tokio::runtime::Builder::new_current_thread()
			.enable_time()
			.build()
			.unwrap()
			.block_on(async {
				let (_sender, ready) = watch::channel(());
				let mut main = tokio::spawn(pending::<Result<(), CriticalError>>());
				main.abort();
				let result = tokio::time::timeout(
					Duration::from_secs(1),
					wait_for_fs_ready(ready, &mut main),
				)
				.await
				.expect("runtime cancellation must not wait for filesystem readiness");
				assert!(result.is_err());
			});
	}

	#[test]
	fn initial_readiness_observes_runtime_exit() {
		tokio::runtime::Builder::new_current_thread()
			.enable_time()
			.build()
			.unwrap()
			.block_on(async {
				let (_sender, ready) = watch::channel(());
				let mut main = tokio::spawn(async { Ok(()) });
				let result = tokio::time::timeout(
					Duration::from_secs(1),
					wait_for_fs_ready(ready, &mut main),
				)
				.await
				.expect("runtime exit must not wait for filesystem readiness");
				assert!(!result.unwrap());
			});
	}

	#[test]
	fn initial_readiness_keeps_the_runtime_running() {
		tokio::runtime::Builder::new_current_thread()
			.enable_time()
			.build()
			.unwrap()
			.block_on(async {
				let (sender, ready) = watch::channel(());
				let mut main = tokio::spawn(pending::<Result<(), CriticalError>>());
				sender.send(()).unwrap();
				assert!(wait_for_fs_ready(ready, &mut main).await.unwrap());
				assert!(!main.is_finished());
				main.abort();
				let _ = main.await;
			});
	}
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
	let (args, _guards) = args::get_args().await?;

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
