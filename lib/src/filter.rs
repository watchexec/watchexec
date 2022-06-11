//! The `Filterer` trait, two implementations, and some helper functions.

use std::sync::Arc;

use crate::{
	error::RuntimeError,
	event::{Event, Priority},
};

pub mod globset;
pub mod tagged;

/// An interface for filtering events.
pub trait Filterer: std::fmt::Debug + Send + Sync {
	/// Called on (almost) every event, and should return `false` if the event is to be discarded.
	///
	/// Checking whether an event passes a filter is synchronous, should be fast, and must not block
	/// the thread. Do any expensive stuff upfront during construction of your filterer, or in a
	/// separate thread/task, as needed.
	///
	/// Returning an error will also fail the event processing, but the error will be propagated to
	/// the watchexec error handler. While the type signature supports any [`RuntimeError`], it's
	/// preferred that you create your own error type and return it wrapped in the
	/// [`RuntimeError::Filterer`] variant with the name of your filterer as `kind`.
	fn check_event(&self, event: &Event, priority: Priority) -> Result<bool, RuntimeError>;
}

impl Filterer for () {
	fn check_event(&self, _event: &Event, _priority: Priority) -> Result<bool, RuntimeError> {
		Ok(true)
	}
}

impl<T: Filterer> Filterer for Arc<T> {
	fn check_event(&self, event: &Event, priority: Priority) -> Result<bool, RuntimeError> {
		Arc::as_ref(self).check_event(event, priority)
	}
}
