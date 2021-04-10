//! CLI utilities.

use std::process::Command;

use crate::config::{Config, ConfigBuilder};

#[deprecated(since = "1.15.0", note = "Config has moved to config::Config")]
pub type Args = Config;

#[deprecated(since = "1.15.0", note = "ConfigBuilder has moved to config::ConfigBuilder")]
pub type ArgsBuilder = ConfigBuilder;

/// Clear the screen.
#[cfg(target_family = "windows")]
pub fn clear_screen() {
// TODO: clearscreen with powershell?
    let _ = Command::new("cmd")
        .arg("/c")
        .arg("tput reset || cls")
        .status();
}

/// Clear the screen.
#[cfg(target_family = "unix")]
pub fn clear_screen() {
// TODO: clear screen via control codes instead
    let _ = Command::new("tput").arg("reset").status();
}

#[deprecated(since = "1.15.0", note = "this will be removed from the library API. use the builder")]
pub use crate::args::get_args;

#[deprecated(since = "1.15.0", note = "this will be removed from the library API. use the builder")]
pub use crate::args::get_args_from;
