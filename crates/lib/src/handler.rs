//! Hook handlers in Watchexec
//!
//! Hook handlers are things that implement `FnMut(T) + Send + 'static`.
//!
//! All hooks in Watchexec expect to run quickly and do no I/O, so async handlers are not
//! appropriate. If you want to do I/O or use async methods, you should architect in such a way
//! that you're not blocking a handler, such as using shared datastructures mutated from a thread
//! that you pass requests to (via channels).
//!
//! Filters are not hooks and can do I/O, see the filtering docs for those.
//!
//! Additionally, hooks in Watchexec don't have error handling: if you need to return errors, use a
//! channel; if you need to crash, then panic. As such all handlers expect to return `()`.
//!
//! # HandlerLock
//!
//! The [`HandlerLock`] structure makes it possible to pass a handler around as a value and most
//! importantly replace it at runtime. Watchexec is based around the idea of being a runtime that
//! is reconfigurable and not static, and this is one part of that puzzle.
//!
//! # Examples
//!
//! Reconfiguring a handler held by a lock:
//!
//! ```
//! # let _ = async || {
//! use watchexec::handler::HandlerLock;
//!
//! let lock = HandlerLock::default();
//!
//! lock.replace(|changed: bool| {
//!        if changed {
//!            println!("something changed!");
//!        }
//! }).await;
//!
//! lock.call(true).await;
//! lock.call(false).await;
//!
//! lock.replace(|changed: bool| {
//!     if !changed {
//!         println!("nothing to see here");
//!     }
//! }).await;
//!
//! lock.call(true).await;
//! lock.call(false).await;
//! # };
//! ```

use std::{
	any::type_name,
	fmt,
	sync::{Arc, Mutex},
};

/// A shareable inner-replaceable wrapper for an FnMut.
///
/// Initialise with `::default()`.
#[allow(clippy::type_complexity)]
pub struct HandlerLock<T>(Arc<Mutex<Box<dyn FnMut(T) + Send>>>);
impl<T> HandlerLock<T>
where
	T: Send,
{
	/// Replace the handler with a new one.
	///
	/// Panics if the lock was poisoned.
	pub fn replace(&self, new: impl FnMut(T) + Send + 'static) {
		let mut handler = self.0.lock().expect("handler lock poisoned");
		*handler = Box::new(new);
	}

	/// Call the handler.
	///
	/// Panics if the lock was poisoned.
	pub fn call(&self, data: T) {
		let mut handler = self.0.lock().expect("handler lock poisoned");
		(handler)(data);
	}
}

impl<T> Clone for HandlerLock<T> {
	fn clone(&self) -> Self {
		Self(Arc::clone(&self.0))
	}
}

impl<T> Default for HandlerLock<T>
where
	T: Send,
{
	fn default() -> Self {
		Self(Arc::new(Mutex::new(Box::new(|_| {}))))
	}
}

impl<T> fmt::Debug for HandlerLock<T> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("HandlerLock")
			.field("payload type", &type_name::<T>())
			.finish_non_exhaustive()
	}
}
