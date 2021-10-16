//! A value that is always available, but can be swapped out.

use std::fmt;

use tokio::sync::watch::{channel, error::SendError, Receiver, Ref, Sender};

/// A value that is always available, but can be swapped out.
///
/// This is a wrapper around a [Tokio `watch`][tokio::sync::watch]. The value can be read without
/// await, but can only be written to with async. Borrows should be held for as little as possible,
/// as they keep a read lock.
pub struct SwapLock<T: Clone> {
	r: Receiver<T>,
	s: Sender<T>,
}

impl<T> SwapLock<T>
where
	T: Clone,
{
	/// Create a new `SwapLock` with the given value.
	pub fn new(inner: T) -> Self {
		let (s, r) = channel(inner);
		Self { r, s }
	}

	/// Get a reference to the value.
	pub fn borrow(&self) -> Ref<'_, T> {
		self.r.borrow()
	}

	/// Rewrite the value using a closure.
	///
	/// This obtains a clone of the value, and then calls the closure with a mutable reference to
	/// it. Once the closure returns, the value is swapped in.
	pub async fn change(&self, f: impl FnOnce(&mut T)) -> Result<(), SendError<T>> {
		let mut new = self.r.borrow().clone();
		f(&mut new);
		self.s.send(new)
	}

	/// Replace the value with a new one.
	pub async fn replace(&self, new: T) -> Result<(), SendError<T>> {
		self.s.send(new)
	}
}

impl<T> fmt::Debug for SwapLock<T>
where
	T: fmt::Debug + Clone,
{
	fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
		f.debug_struct("SwapLock")
			.field("(watch)", &self.r)
			.finish()
	}
}
