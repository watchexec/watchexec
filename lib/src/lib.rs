//! Watchexec: a library for utilities and programs which respond to events;
//! file changes, human interaction, and more.
//!
//! Also see the CLI tool: https://watchexec.github.io/
//!
//! This library is powered by [Tokio](https://tokio.rs), minimum version 1.10.
//!
//! The main way to use this crate involves constructing a [`Handler`] and running it.
//!
//! This crate does not itself use `unsafe`. However, it depends on a number of libraries which do,
//! most because they interact with the operating system.

#![doc(html_favicon_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![doc(html_logo_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![warn(clippy::unwrap_used)]
#![forbid(unsafe_code)]

pub mod error;
pub mod event;
pub mod fs;
pub mod shell;
pub mod signal;

// the *action* is debounced, not the events
