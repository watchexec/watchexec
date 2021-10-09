//! Watchexec: a library for utilities and programs which respond to events;
//! file changes, human interaction, and more.
//!
//! Also see the CLI tool: <https://watchexec.github.io/>
//!
//! This library is powered by [Tokio](https://tokio.rs), minimum version 1.10. This requirement may
//! change (upwards) in the future without breaking change.
//!
//! The main way to use this crate involves constructing a [`Watchexec`] around an
//! [`InitConfig`][config::InitConfig] and a [`RuntimeConfig`][config::RuntimeConfig], then running
//! it. [`Handler`][handler::Handler]s are used to hook into watchexec at various points. The
//! runtime config can be changed at any time with the [`Watchexec::reconfigure()`] method.
//!
//! ```no_run
//! # use color_eyre::eyre::Report;
//! # use std::convert::Infallible;
//! use watchexec::{
//!     Watchexec,
//!     action::{Action, Outcome},
//!     config::{InitConfig, RuntimeConfig},
//!     handler::{Handler as _, PrintDebug},
//! };
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Report> {
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
//!             for event in &action.events {
//!                 if event.paths().any(|p| p.ends_with("/watchexec.conf")) {
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
//! #           as Result<_, Infallible>
//!         }
//!     });
//!
//!     we.main().await?;
//!     Ok(())
//! }
//! # struct YourConfigFormat;
//! # impl YourConfigFormat {
//! # async fn load_from_file(_: &str) -> Result<Self, Infallible> { Ok::<_, Infallible>(Self) }
//! # fn apply(&self, _: &mut RuntimeConfig) { }
//! # }
//! ```
//!
//! Alternatively, one can use the modules exposed by the crate and the external crates such as
//! [ClearScreen][clearscreen] and [Command Group][command_group] to build something more advanced,
//! at the cost of reimplementing the glue code. See the examples folder for some basic/demo tools
//! written with the individual modules.
//!
//! This crate does not itself use `unsafe`. However, it depends on a number of libraries which do,
//! most because they interact with the operating system.

#![doc(html_favicon_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![doc(html_logo_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![warn(clippy::unwrap_used)]
#![forbid(unsafe_code)]

// the toolkit to make your own
pub mod action;
pub mod command;
pub mod error;
pub mod event;
pub mod filter;
pub mod fs;
pub mod ignore_files;
pub mod project;
pub mod signal;

// the core experience
pub mod config;
pub mod handler;
mod watchexec;

#[doc(inline)]
pub use crate::watchexec::Watchexec;

// the *action* is debounced, not the events
