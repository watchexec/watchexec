//! [Watchexec]: the library
//!
//! From version 1.16.0, semver applies!
//!
//! [Watchexec]: https://github.com/watchexec/watchexec

#![doc(html_favicon_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![doc(html_logo_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![warn(clippy::unwrap_used)]

#[macro_use]
extern crate derive_builder;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

pub mod config;
pub mod error;
mod gitignore;
mod ignore;
mod notification_filter;
pub mod pathop;
mod process;
pub mod run;
mod signal;
mod watcher;

pub use process::Shell;
pub use run::{run, watch, Handler};
