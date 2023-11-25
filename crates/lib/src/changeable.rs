//! Changeable values.

use std::{
	any::type_name,
	fmt,
	sync::{Arc, RwLock},
};

/// A shareable value that doesn't keep a lock when it is read.
///
/// This is essentially an `Arc<RwLock<T: Clone>>`, with the only two methods to use it as:
/// - replace the value, which obtains a write lock
/// - get a clone of that value, which obtains a read lock
///
/// but importantly because you get a clone of the value, the read lock is not held after the
/// `get()` method returns.
///
/// See [`ChangeableFn`] for a specialised variant which holds an [`Fn`].
#[derive(Clone)]
pub struct Changeable<T>(Arc<RwLock<T>>);
impl<T> Changeable<T>
where
	T: Clone + Send,
{
	/// Create a new Changeable.
	///
	/// If `T: Default`, prefer using `::default()`.
	#[must_use]
	pub fn new(value: T) -> Self {
		Self(Arc::new(RwLock::new(value)))
	}

	/// Replace the value with a new one.
	///
	/// Panics if the lock was poisoned.
	pub fn replace(&self, new: T) {
		*(self.0.write().expect("changeable lock poisoned")) = new;
	}

	/// Get a clone of the value.
	///
	/// Panics if the lock was poisoned.
	#[must_use]
	pub fn get(&self) -> T {
		self.0.read().expect("handler lock poisoned").clone()
	}
}

impl<T> Default for Changeable<T>
where
	T: Clone + Send + Default,
{
	fn default() -> Self {
		Self::new(T::default())
	}
}

// TODO: with specialisation, write a better impl when T: Debug
impl<T> fmt::Debug for Changeable<T> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("Changeable")
			.field("inner type", &type_name::<T>())
			.finish_non_exhaustive()
	}
}

/// A shareable `Fn` that doesn't hold a lock when it is called.
///
/// This is a specialisation of [`Changeable`] for the `Fn` usecase.
///
/// As this is for Watchexec, only `Fn`s with a single argument and return value are supported
/// here; it's simple enough to make your own if you want more.
pub struct ChangeableFn<T, U>(Changeable<Arc<dyn (Fn(T) -> U) + Send + Sync>>);
impl<T, U> ChangeableFn<T, U>
where
	T: Send,
	U: Send,
{
	pub(crate) fn new(f: impl (Fn(T) -> U) + Send + Sync + 'static) -> Self {
		Self(Changeable::new(Arc::new(f)))
	}

	/// Replace the fn with a new one.
	///
	/// Panics if the lock was poisoned.
	pub fn replace(&self, new: impl (Fn(T) -> U) + Send + Sync + 'static) {
		self.0.replace(Arc::new(new));
	}

	/// Call the fn.
	///
	/// Panics if the lock was poisoned.
	pub fn call(&self, data: T) -> U {
		(self.0.get())(data)
	}
}

// the derive adds a T: Clone bound
impl<T, U> Clone for ChangeableFn<T, U> {
	fn clone(&self) -> Self {
		Self(Changeable::clone(&self.0))
	}
}

impl<T, U> Default for ChangeableFn<T, U>
where
	T: Send,
	U: Send + Default,
{
	fn default() -> Self {
		Self::new(|_| U::default())
	}
}

impl<T, U> fmt::Debug for ChangeableFn<T, U> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("ChangeableFn")
			.field("payload type", &type_name::<T>())
			.field("return type", &type_name::<U>())
			.finish_non_exhaustive()
	}
}
