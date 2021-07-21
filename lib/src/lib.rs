//! [Watchexec]: the library
//!
//! From version 1.16.0, semver applies!
//!
//! [Watchexec]: https://github.com/watchexec/watchexec

#![doc(html_favicon_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![doc(html_logo_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![warn(clippy::unwrap_used)]

pub mod config;
pub mod error;
mod gitignore;
mod ignore;
mod notification_filter;
pub mod pathop;
mod paths;
pub mod run;
mod shell;
mod signal;
mod watcher;

pub use run::{run, watch, Handler};
pub use shell::Shell;
