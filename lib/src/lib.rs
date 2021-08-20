//! Watchexec: a library for utilities and programs which respond to events;
//! file changes, human interaction, and more.
//!
//! Also see the CLI tool: <https://watchexec.github.io/>
//!
//! This library is powered by [Tokio](https://tokio.rs), minimum version 1.10. This requirement may
//! change (upwards) in the future without breaking change.
//!
//! The main way to use this crate involves constructing a [`Watchexec`] around a [`Config`] and
//! running it. The config may contain some instances of [`Handler`][handler::Handler]s, hooking
//! into watchexec at various points.
//!
//! ```ignore // TODO: implement and switch to no_run
//! use watchexec::{Watchexec, ConfigBuilder, Handler as _};
//!
//! #[tokio::main]
//! async fn main() {
//!     let mut config = ConfigBuilder::new()
//!     config.pathset(["watchexec.conf"]);
//!
//!     let conf = YourConfigFormat::load_from_file("watchexec.conf").await?;
//!     conf.apply(&mut config);
//!
//!     let we = Watchexec::new(config.build().unwrap()).unwrap();
//!     let w = we.clone();
//!
//!     let c = config.clone();
//!     config.on_event(|e| async move {
//!         if e.path().map(|p| p.ends_with("watchexec.conf")).unwrap_or(false) {
//!             let conf = YourConfigFormat::load_from_file("watchexec.conf").await?;
//!
//!             conf.apply(&mut config);
//!             w.reconfigure(config.build());
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
pub mod command;
pub mod error;
pub mod event;
pub mod fs;
pub mod handler;
pub mod signal;

// the core experience
mod config;
mod watchexec;

#[doc(inline)]
pub use crate::watchexec::Watchexec;
#[doc(inline)]
pub use config::{Config, ConfigBuilder};

// the *action* is debounced, not the events
