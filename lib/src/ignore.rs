//! Ignore files: find them, parse them, interpret them.

#[doc(inline)]
pub use files::*;
#[doc(inline)]
pub use filter::*;

mod files;
mod filter;
pub mod tree;
