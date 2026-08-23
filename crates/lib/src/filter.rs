//! The `Filterer` trait for event filtering.

use std::{fmt, path::Path, sync::Arc};

use watchexec_events::{Event, Priority};

use crate::{changeable::Changeable, error::RuntimeError};

/// An interface for filtering events.
pub trait Filterer: std::fmt::Debug + Send + Sync {
	/// Called while reconciling filesystem event sources to decide whether a directory should be
	/// watched.
	///
	/// This is source filtering, which is separate from [`Filterer::check_event`]. Returning `false`
	/// excludes the directory as an event source, including its descendants; `check_event` still
	/// filters events which are observed from accepted sources. Implementations should therefore
	/// reject a directory only when every event beneath it can safely be ignored.
	///
	/// An exact configured watch root is never passed to this method. The root remains an event
	/// source even if this method would reject it, while its descendants are checked normally.
	/// Watchexec only calls this method on filesystem backends for which it manages recursion; see
	/// [`crate::sources::fs`] for the backend-specific behaviour.
	///
	/// Like event filtering, source-directory filtering is synchronous, should be fast, and must not
	/// block the thread.
	///
	/// The default implementation accepts every directory.
	fn check_dir(&self, _path: &Path) -> Result<bool, RuntimeError> {
		Ok(true)
	}

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
	fn check_dir(&self, _path: &Path) -> Result<bool, RuntimeError> {
		Ok(true)
	}

	fn check_event(&self, _event: &Event, _priority: Priority) -> Result<bool, RuntimeError> {
		Ok(true)
	}
}

impl<T: Filterer + ?Sized> Filterer for Arc<T> {
	fn check_dir(&self, path: &Path) -> Result<bool, RuntimeError> {
		Self::as_ref(self).check_dir(path)
	}

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
	/// This type does not know whether it belongs to a [`Config`](crate::Config), so calling this on
	/// `Config::filterer` does not emit the config change signal by itself. Prefer
	/// [`Config::filterer`](crate::Config::filterer), or call
	/// [`Config::signal_change`](crate::Config::signal_change) after direct replacement so filesystem
	/// sources are reconciled.
	///
	/// Panics if the lock was poisoned.
	pub fn replace(&self, new: impl Filterer + 'static) {
		self.0.replace(Arc::new(new));
	}

	/// Get a stable snapshot of the current filterer.
	///
	/// The snapshot remains valid if the configured filterer is replaced, and its pointer identity
	/// can be compared with later snapshots to detect a replacement.
	#[must_use]
	pub(crate) fn snapshot(&self) -> Arc<dyn Filterer> {
		self.0.get()
	}
}

impl Filterer for ChangeableFilterer {
	fn check_dir(&self, path: &Path) -> Result<bool, RuntimeError> {
		self.snapshot().check_dir(path)
	}

	fn check_event(&self, event: &Event, priority: Priority) -> Result<bool, RuntimeError> {
		self.snapshot().check_event(event, priority)
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
