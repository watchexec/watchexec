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

/// Helper trait to construct specific IO errors from generic ones.
pub trait SpecificIoError<Output> {
	/// Add some context to the error or result.
	fn about(self, context: &'static str) -> Output;
}
