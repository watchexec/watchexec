#[macro_use]
extern crate clap;
extern crate globset;
extern crate env_logger;
extern crate libc;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
extern crate notify;

#[cfg(unix)]
extern crate nix;
#[cfg(windows)]
extern crate winapi;
#[cfg(windows)]
extern crate kernel32;

#[cfg(test)]
extern crate mktemp;

pub mod cli;
mod gitignore;
mod notification_filter;
mod process;
pub mod run;
mod signal;
mod watcher;

pub use run::run;
