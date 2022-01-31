use std::{
	sync::{Arc, Mutex},
	time::Duration,
};

use notify::Watcher as _;

use crate::error::RuntimeError;

/// What kind of filesystem watcher to use.
///
/// For now only native and poll watchers are supported. In the future there may be additional
/// watchers available on some platforms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Watcher {
	/// The Notify-recommended watcher on the platform.
	///
	/// For platforms Notify supports, that's a [native implementation][notify::RecommendedWatcher],
	/// for others it's polling with a default interval.
	Native,

	/// Notify’s [poll watcher][notify::PollWatcher] with a custom interval.
	Poll(Duration),
}

impl Default for Watcher {
	fn default() -> Self {
		Self::Native
	}
}

impl Watcher {
	pub(super) fn create(
		self,
		f: impl notify::EventHandler,
	) -> Result<Box<dyn notify::Watcher + Send>, RuntimeError> {
		match self {
			Self::Native => notify::RecommendedWatcher::new(f).map(|w| Box::new(w) as _),
			Self::Poll(delay) => notify::PollWatcher::with_delay(Arc::new(Mutex::new(f)), delay)
				.map(|w| Box::new(w) as _),
		}
		.map_err(|err| RuntimeError::FsWatcherCreate {
			kind: self,
			help: if cfg!(target_os = "linux") && (matches!(err.kind, notify::ErrorKind::MaxFilesWatch) || matches!(err.kind, notify::ErrorKind::Io(ref ioerr) if ioerr.raw_os_error() == Some(28))) {
				"you will want to increase your inotify.max_user_watches, see inotify(7) and https://watchexec.github.io/docs/inotify-limits.html"
			} else if cfg!(target_os = "linux") && matches!(err.kind, notify::ErrorKind::Io(ref ioerr) if ioerr.raw_os_error() == Some(24)) {
				"you will want to increase your `nofile` limit, see pam_limits(8)"
			} else {
				"you may want to try again with the polling watcher"
			}.into(),
			err,
		})
	}
}
