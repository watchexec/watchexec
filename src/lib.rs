//! Watchexec: the library
//!
//! This is the library version of the CLI tool [watchexec]. The tool is
//! implemented with this library, but the purpose of the watchexec project is
//! to deliver the CLI tool, instead of focusing on the library interface first
//! and foremost. **For this reason, semver guarantees do _not_ apply to this
//! library.** Please use exact version matching, as this API may break even
//! between patch point releases. This policy may change in the future.
//!
//! [watchexec]: https://github.com/watchexec/watchexec

#![forbid(deprecated)]
#![warn(
    clippy::all,
    clippy::missing_const_for_fn,
    clippy::option_unwrap_used,
    clippy::result_unwrap_used,
    intra_doc_link_resolution_failure
)]

#[macro_use]
extern crate clap;
#[macro_use]
extern crate derive_builder;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

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

pub use cli::{Args, ArgsBuilder};
pub use run::{run, watch, Handler};
