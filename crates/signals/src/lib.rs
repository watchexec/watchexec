//! Signal types for Watchexec.
//!
//! There are two types of signals:
//! - [`MainSignal`][MainSignal] is a signal sent to the main process.
//! - [`SubSignal`][SubSignal] is a signal sent to or received from a sub process.
//!
//! ## Features
//!
//! - `parse`: Enables parsing of signals from strings.
//! - `miette`: Enables [`miette`][miette] support for [`SignalParseError`][SignalParseError].

#[doc(inline)]
pub use dom::*;

#[doc(inline)]
pub use sub::*;

mod dom;
mod sub;
