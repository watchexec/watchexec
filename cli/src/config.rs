use clap::ArgMatches;
use color_eyre::eyre::{eyre, Result};
use watchexec::{
	command::Shell,
	config::{InitConfig, RuntimeConfig},
};

pub fn new(args: &ArgMatches<'static>) -> Result<(InitConfig, RuntimeConfig)> {
	Ok((init(&args)?, runtime(&args)?))
}

fn init(args: &ArgMatches<'static>) -> Result<InitConfig> {
    let mut config = InitConfig::builder();

    Ok(config.build()?)
}

fn runtime(args: &ArgMatches<'static>) -> Result<RuntimeConfig> {
    let mut config = RuntimeConfig::default();


    Ok(config)
}

// until 2.0
#[cfg(windows)]
fn default_shell() -> Shell {
	Shell::Cmd
}

#[cfg(not(windows))]
fn default_shell() -> Shell {
	Shell::default()
}

// because Shell::Cmd is only on windows
#[cfg(windows)]
fn cmd_shell(_: String) -> Shell {
	Shell::Cmd
}

#[cfg(not(windows))]
fn cmd_shell(s: String) -> Shell {
	Shell::Unix(s)
}
