//! Event source for changes to files and directories.

#[doc(inline)]
pub use watcher::*;
#[doc(inline)]
pub use worker::*;
#[doc(inline)]
pub use workingdata::*;

mod watcher;
mod worker;
mod workingdata;
