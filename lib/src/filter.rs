//! The `Filterer` trait, two implementations, and some helper functions.

use std::sync::Arc;

use ignore::gitignore::GitignoreBuilder;

use crate::{
	error::{GlobParseError, RuntimeError},
	event::Event,
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
	fn check_event(&self, event: &Event) -> Result<bool, RuntimeError>;
}

impl Filterer for () {
	fn check_event(&self, _event: &Event) -> Result<bool, RuntimeError> {
		Ok(true)
	}
}

impl<T: Filterer> Filterer for Arc<T> {
	fn check_event(&self, event: &Event) -> Result<bool, RuntimeError> {
		Arc::as_ref(self).check_event(event)
	}
}

/// Convenience function to check a glob pattern from a string.
///
/// This parses the glob and wraps any error with nice [miette] diagnostics.
pub fn check_glob(glob: &str) -> Result<(), GlobParseError> {
	let mut builder = GitignoreBuilder::new("/");
	if let Err(err) = builder.add_line(None, glob) {
		if let ignore::Error::Glob { err, .. } = err {
			// TODO: use globset and return a nicer error
			Err(GlobParseError::new(glob, &err))
		} else {
			Err(GlobParseError::new(glob, "unknown error"))
		}
	} else {
		Ok(())
	}
}
