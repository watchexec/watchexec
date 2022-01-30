use std::convert::Infallible;

use clap::ArgMatches;
use miette::{Report, Result};
use tracing::error;
use watchexec::{config::InitConfig, error::RuntimeError, handler::SyncFnHandler, ErrorHook};

pub fn init(_args: &ArgMatches<'static>) -> Result<InitConfig> {
	let mut config = InitConfig::default();
	config.on_error(SyncFnHandler::from(
		|err: ErrorHook| -> std::result::Result<(), Infallible> {
			if let RuntimeError::IoError {
				about: "waiting on process group",
				..
			} = err.error
			{
				// "No child processes" and such
				// these are often spurious, so condemn them to -v only
				error!("{}", err.error);
				return Ok(());
			}

			if let RuntimeError::FsWatcherCreate { .. } = err.error {
				err.elevate();
				return Ok(());
			}

			if cfg!(debug_assertions) {
				eprintln!("[[{:?}]]", err.error);
			}

			eprintln!("[[Error (not fatal)]]\n{}", Report::new(err.error));

			Ok(())
		},
	));

	Ok(config)
}
