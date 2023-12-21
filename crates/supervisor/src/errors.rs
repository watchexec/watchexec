//! Error types.

use std::{
	io::Error,
	sync::{Arc, OnceLock},
};

/// Convenience type for a [`std::io::Error`] which can be shared across threads.
pub type SyncIoError = Arc<OnceLock<Error>>;

/// Make a [`SyncIoError`] from a [`std::io::Error`].
#[must_use]
pub fn sync_io_error(err: Error) -> SyncIoError {
	let lock = OnceLock::new();
	lock.set(err).expect("unreachable: lock was just created");
	Arc::new(lock)
}
