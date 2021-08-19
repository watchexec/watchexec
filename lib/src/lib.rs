//! Watchexec: a library for utilities and programs which respond to events;
//! file changes, human interaction, and more.
//!
//! Also see the CLI tool: <https://watchexec.github.io/>
//!
//! This library is powered by [Tokio](https://tokio.rs), minimum version 1.10. This requirement may
//! change (upwards) in the future without breaking change.
//!
//! The main way to use this crate involves constructing a [`Watchexec`] around a [`Config`] and
//! running it. The config may contain some instances of [`Handler`]s, which hook into watchexec
//! processing at various points.
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
pub mod error;
pub mod event;
pub mod fs;
pub mod command;
pub mod signal;

// the core experience
mod config;
mod handler;
mod watchexec;

#[doc(inline)]
pub use config::Config;
#[doc(inline)]
pub use handler::Handler;
#[doc(inline)]
pub use watchexec::Watchexec;

// the *action* is debounced, not the events
