use std::io::stderr;

use clap::ArgMatches;
use miette::Result;
use watchexec::{config::InitConfig, handler::PrintDisplay};

pub fn init(_args: &ArgMatches<'static>) -> Result<InitConfig> {
	let mut config = InitConfig::default();
	config.on_error(PrintDisplay(stderr()));
	Ok(config)
}
