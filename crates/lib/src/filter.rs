//! The `Filterer` trait for event filtering.

use std::{fmt, sync::Arc};

use watchexec_events::{Event, Priority};

use crate::{changeable::Changeable, error::RuntimeError};

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
		Self::as_ref(self).check_event(event, priority)
	}
}

/// A shareable `Filterer` that doesn't hold a lock when it is called.
///
/// This is a specialisation of [`Changeable`] for `Filterer`.
pub struct ChangeableFilterer(Changeable<Arc<dyn Filterer>>);
impl ChangeableFilterer {
	/// Replace the filterer with a new one.
	///
	/// Panics if the lock was poisoned.
	pub fn replace(&self, new: impl Filterer + Send + Sync + 'static) {
		self.0.replace(Arc::new(new));
	}
}

impl Filterer for ChangeableFilterer {
	fn check_event(&self, event: &Event, priority: Priority) -> Result<bool, RuntimeError> {
		Arc::as_ref(&self.0.get()).check_event(event, priority)
	}
}

// the derive adds a T: Clone bound
impl Clone for ChangeableFilterer {
	fn clone(&self) -> Self {
		Self(Changeable::clone(&self.0))
	}
}

impl Default for ChangeableFilterer {
	fn default() -> Self {
		Self(Changeable::new(Arc::new(())))
	}
}

impl fmt::Debug for ChangeableFilterer {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("ChangeableFilterer")
			.field("filterer", &format!("{:?}", self.0.get()))
			.finish_non_exhaustive()
	}
}
