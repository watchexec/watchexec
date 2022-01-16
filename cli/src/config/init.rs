use std::convert::Infallible;

use clap::ArgMatches;
use miette::Result;
use tracing::error;
use watchexec::{config::InitConfig, error::RuntimeError, handler::SyncFnHandler};

pub fn init(_args: &ArgMatches<'static>) -> Result<InitConfig> {
	let mut config = InitConfig::default();
	config.on_error(SyncFnHandler::from(
		|data| -> std::result::Result<(), Infallible> {
			// if let RuntimeError::IoError { .. } = data {
			// 	// these are often spurious, so condemn them to -v only
			// 	error!("{}", data);
			// 	return Ok(());
			// }

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
