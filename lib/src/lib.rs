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
//! runtime config can be changed at any time with the [`reconfig()`][Watchexec::reconfig()] method.
//!
//! ```ignore // TODO: implement and switch to no_run
//! use watchexec::{Watchexec, InitConfigBuilder, RuntimeConfigBuilder, Handler as _};
//!
//! #[tokio::main]
//! async fn main() {
//!     let init = InitConfigBuilder::default()
//!         .error_handler(PrintDebug(std::io::stderr()));
//!
//!     let mut runtime = RuntimeConfigBuilder::default()
//!     config.pathset(["watchexec.conf"]);
//!
//!     let conf = YourConfigFormat::load_from_file("watchexec.conf").await?;
//!     conf.apply(&mut runtime);
//!
//!     let we = Watchexec::new(init.build().unwrap(), runtime.build().unwrap()).unwrap();
//!     let w = we.clone();
//!
//!     let c = config.clone();
//!     config.on_event(|e| async move {
//!         if e.path().map(|p| p.ends_with("watchexec.conf")).unwrap_or(false) {
//!             let conf = YourConfigFormat::load_from_file("watchexec.conf").await?;
//!
//!             conf.apply(&mut runtime);
//!             w.reconfigure(runtime.build().unwrap());
//!             // tada! self-reconfiguring watchexec on config file change!
//!         }
//!     });
//!
//!     w.main().await.unwrap();
//! }
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
pub mod fs;
pub mod signal;

// the core experience
pub mod config;
pub mod handler;
mod watchexec;

#[doc(inline)]
pub use crate::watchexec::Watchexec;

// the *action* is debounced, not the events
