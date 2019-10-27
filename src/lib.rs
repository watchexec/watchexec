//! Watchexec: the library
//!
//! This is the library version of the CLI tool [watchexec]. The tool is
//! implemented with this library, but the purpose of the watchexec project is
//! to deliver the CLI tool, instead of focusing on the library interface first
//! and foremost. **For this reason, semver guarantees do _not_ apply to this
//! library.** Please use exact version matching, as this API may break even
//! between patch point releases.
//!
//! [watchexec]: https://github.com/watchexec/watchexec

#[macro_use]
extern crate clap;
#[macro_use]
extern crate derive_builder;
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

pub mod cli;
pub mod error;
mod gitignore;
mod ignore;
mod notification_filter;
pub mod pathop;
mod process;
pub mod run;
mod signal;
mod watcher;

pub use run::run;
