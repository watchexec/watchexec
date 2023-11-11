use std::{
	io::Error,
	sync::{Arc, OnceLock},
};

pub type SyncIoError = Arc<OnceLock<Error>>;

pub fn sync_io_error(err: Error) -> SyncIoError {
	let lock = OnceLock::new();
	lock.set(err).expect("unreachable: lock was just created");
	Arc::new(lock)
}
