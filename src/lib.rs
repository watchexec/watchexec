#[macro_use]
extern crate clap;
extern crate env_logger;
extern crate globset;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
extern crate notify;

#[cfg(windows)]
extern crate kernel32;
#[cfg(unix)]
extern crate nix;
#[cfg(windows)]
extern crate winapi;

#[cfg(test)]
extern crate mktemp;

pub mod cli;
pub mod error;
mod gitignore;
mod notification_filter;
pub mod pathop;
mod process;
pub mod run;
mod signal;
mod watcher;

pub use run::run;
