use std::convert::Infallible;

use clap::ArgMatches;
use miette::Result;
use tracing::error;
use watchexec::{config::InitConfig, error::RuntimeError, handler::SyncFnHandler};

pub fn init(_args: &ArgMatches<'static>) -> Result<InitConfig> {
	let mut config = InitConfig::default();
	config.on_error(SyncFnHandler::from(
		|data| -> std::result::Result<(), Infallible> {
			if let RuntimeError::IoError {
				about: "waiting on process group",
				..
			} = data
			{
				// "No child processes" and such
				// these are often spurious, so condemn them to -v only
				error!("{}", data);
				return Ok(());
			}

			if cfg!(debug_assertions) {
				eprintln!("[[{:?}]]", data);
			} else {
				eprintln!("[[{}]]", data);
			}

			Ok(())
		},
	));

	Ok(config)
}
