//! Watchexec: a library for utilities and programs which respond to (file, signal, etc) events
//! primarily by launching or managing other programs.
//!
//! Also see the CLI tool: <https://watchexec.github.io/>
//!
//! This library is powered by [Tokio](https://tokio.rs).
//!
//! The main way to use this crate involves constructing a [`Watchexec`] around a [`Config`], then
//! running it. [`Handler`][handler::Handler]s are used to hook into Watchexec at various points.
//! The config can be changed at any time with the [`Watchexec::reconfigure()`] method.
//!
//! It's recommended to use the [miette] erroring library in applications, but all errors implement
//! [`std::error::Error`] so your favourite error handling library can of course be used.
//!
//! ```no_run
//! use miette::{IntoDiagnostic, Result};
//! use watchexec_signals::Signal;
//! use watchexec::{action::Action, Watchexec};
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let wx = Watchexec::new(|action: Action| {
//!         // print any events
//!         for event in action.events.iter() {
//!             eprintln!("EVENT: {event:?}");
//!         }
//!
//!         // if Ctrl-C is received, quit
//!         if action.signals().any(|sig| sig == Signal::Interrupt) {
//!             action.quit();
//!         }
//!     })?;
//!
//!     // watch the current directory
//!     wx.config.pathset(["."]);
//!
//!     wx.main().await.into_diagnostic()?;
//!     Ok(())
//! }
//! ```
//!
//! Alternatively, you can use the modules exposed by the crate and the external crates such as
//! [ClearScreen][clearscreen] and [Command Group][command_group] to build something more advanced,
//! at the cost of reimplementing the glue code.
//!
//! Note that the library generates a _lot_ of debug messaging with [tracing]. **You should not
//! enable printing even `error`-level log messages for this crate unless it's for debugging.**
//! Instead, make use of the [`Config::on_error()`] method to define a handler for errors
//! occurring at runtime that are _meant_ for you to handle (by printing out or otherwise).

#![doc(html_favicon_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![doc(html_logo_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![warn(clippy::unwrap_used, missing_docs)]
#![deny(rust_2018_idioms)]

// the toolkit to make your own
pub mod action;
pub mod command;
pub mod error;
pub mod filter;
pub mod fs;
pub mod keyboard;
pub mod paths;
pub mod signal;

// the core experience
pub mod changeable;
pub mod config;
mod watchexec;

#[doc(inline)]
pub use crate::watchexec::{ErrorHook, Watchexec};

pub use crate::config::Config;

#[cfg(debug_assertions)]
#[doc(hidden)]
pub mod readme_doc_check {
	#[doc = include_str!("../README.md")]
	pub struct Readme;
}
