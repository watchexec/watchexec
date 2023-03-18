#![doc = include_str!("../README.md")]

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
