//! Error types for critical, runtime, and specialised errors.

#[doc(inline)]
pub use critical::*;
#[doc(inline)]
pub use runtime::*;
#[doc(inline)]
pub use specialised::*;

mod critical;
mod runtime;
mod specialised;
