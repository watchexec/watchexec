//! Synthetic event type, derived from inputs, triggers actions.
//!
//! Fundamentally, events in watchexec have three purposes:
//!
//! 1. To trigger the launch, restart, or other interruption of a process;
//! 2. To be filtered upon according to whatever set of criteria is desired;
//! 3. To carry information about what caused the event, which may be provided to the process.

#[doc(inline)]
pub use event::*;

#[doc(inline)]
pub use fs::*;

#[doc(inline)]
pub use keyboard::*;

#[doc(inline)]
pub use process::*;

mod event;
mod fs;
mod keyboard;
mod process;

#[cfg(not(feature = "notify"))]
mod sans_notify;

#[cfg(feature = "serde")]
mod serde_formats;
