//! Watchexec: a library for utilities and programs which respond to events;
//! file changes, human interaction, and more.
//!
//! Also see the CLI tool: <https://watchexec.github.io/>
//!
//! This library is powered by [Tokio](https://tokio.rs).
//!
//! The main way to use this crate involves constructing a [`Watchexec`] around an
//! [`InitConfig`][config::InitConfig] and a [`RuntimeConfig`][config::RuntimeConfig], then running
//! it. [`Handler`][handler::Handler]s are used to hook into watchexec at various points. The
//! runtime config can be changed at any time with the [`Watchexec::reconfigure()`] method.
//!
//! It's recommended to use the [miette] erroring library in applications, but all errors implement
//! [`std::error::Error`] so your favourite error handling library can of course be used.
//!
//! ```no_run
//! use miette::{IntoDiagnostic, Result};
//! use watchexec::{
//!     Watchexec,
//!     action::{Action, Outcome},
//!     config::{InitConfig, RuntimeConfig},
//!     handler::{Handler as _, PrintDebug},
//! };
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let mut init = InitConfig::default();
//!     init.on_error(PrintDebug(std::io::stderr()));
//!
//!     let mut runtime = RuntimeConfig::default();
//!     runtime.pathset(["watchexec.conf"]);
//!
//!     let conf = YourConfigFormat::load_from_file("watchexec.conf").await?;
//!     conf.apply(&mut runtime);
//!
//!     let we = Watchexec::new(init, runtime.clone())?;
//!     let w = we.clone();
//!
//!     let c = runtime.clone();
//!     runtime.on_action(move |action: Action| {
//!         let mut c = c.clone();
//!         let w = w.clone();
//!         async move {
//!             for event in action.events.iter() {
//!                 if event.paths().any(|(p, _)| p.ends_with("/watchexec.conf")) {
//!                     let conf = YourConfigFormat::load_from_file("watchexec.conf").await?;
//!
//!                     conf.apply(&mut c);
//!                     w.reconfigure(c.clone());
//!                     // tada! self-reconfiguring watchexec on config file change!
//!
//!                     break;
//!                 }
//!             }
//!
//!             action.outcome(Outcome::if_running(
//!                 Outcome::DoNothing,
//!                 Outcome::both(Outcome::Clear, Outcome::Start),
//!             ));
//!
//!             Ok(())
//! #           as std::result::Result<_, MietteStub>
//!         }
//!     });
//!
//!     we.main().await.into_diagnostic()?;
//!     Ok(())
//! }
//! # struct YourConfigFormat;
//! # impl YourConfigFormat {
//! # async fn load_from_file(_: &str) -> std::result::Result<Self, MietteStub> { Ok(Self) }
//! # fn apply(&self, _: &mut RuntimeConfig) { }
//! # }
//! # use miette::Diagnostic;
//! # use thiserror::Error;
//! # #[derive(Debug, Error, Diagnostic)]
//! # #[error("stub")]
//! # struct MietteStub;
//! ```
//!
//! Alternatively, one can use the modules exposed by the crate and the external crates such as
//! [ClearScreen][clearscreen] and [Command Group][command_group] to build something more advanced,
//! at the cost of reimplementing the glue code. See the examples folder for some basic/demo tools
//! written with the individual modules.
//!
//! Note that the library generates a _lot_ of debug messaging with [tracing]. You should not enable
//! printing even error log messages for this crate unless it's for debugging. Instead, make use of
//! the [`InitConfig::on_error()`][config::InitConfig::on_error()] method to define a handler for
//! errors occurring at runtime that are _meant_ for you to handle (by printing out or otherwise).
//!
//! This crate does not itself use `unsafe`. However, it depends on a number of libraries which do,
//! most because they interact with the operating system.

#![doc(html_favicon_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![doc(html_logo_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![warn(clippy::unwrap_used, missing_docs)]
#![cfg_attr(not(target_os = "fuchsia"), forbid(unsafe_code))]
// see event::ProcessEnd for why this is disabled on fuchsia

// the toolkit to make your own
pub mod action;
pub mod command;
pub mod error;
pub mod event;
pub mod filter;
pub mod fs;
pub mod ignore;
pub mod paths;
pub mod project;
pub mod signal;

// the core experience
pub mod config;
pub mod handler;
mod watchexec;

#[doc(inline)]
pub use crate::watchexec::Watchexec;
