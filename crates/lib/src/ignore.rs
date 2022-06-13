//! Ignore files: find them, parse them, interpret them.

#[doc(inline)]
pub use discover::*;
#[doc(inline)]
pub use filter::*;
#[doc(inline)]
pub use filterer::*;

mod discover;
mod filter;
mod filterer;
