use std::convert::Infallible;

use miette::Report;
use tracing::error;
use watchexec::{
	config::InitConfig,
	error::{FsWatcherError, RuntimeError},
	handler::SyncFnHandler,
	ErrorHook,
};

use crate::args::Args;

pub fn init(_args: &Args) -> InitConfig {
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

			if let RuntimeError::FsWatcher {
				err:
					FsWatcherError::Create { .. }
					| FsWatcherError::TooManyWatches { .. }
					| FsWatcherError::TooManyHandles { .. },
				..
			} = err.error
			{
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

	config
}
