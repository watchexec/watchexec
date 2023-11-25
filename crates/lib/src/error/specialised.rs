use std::path::PathBuf;

use miette::Diagnostic;
use thiserror::Error;

/// Errors emitted by the filesystem watcher.
#[derive(Debug, Diagnostic, Error)]
#[non_exhaustive]
pub enum FsWatcherError {
	/// Error received when creating a filesystem watcher fails.
	///
	/// Also see `TooManyWatches` and `TooManyHandles`.
	#[error("failed to instantiate")]
	#[diagnostic(help("perhaps retry with the poll watcher"))]
	Create(#[source] notify::Error),

	/// Error received when creating or updating a filesystem watcher fails because there are too many watches.
	///
	/// This is the OS error 28 on Linux.
	#[error("failed to instantiate: too many watches")]
	#[cfg_attr(target_os = "linux", diagnostic(help("you will want to increase your inotify.max_user_watches, see inotify(7) and https://watchexec.github.io/docs/inotify-limits.html")))]
	#[cfg_attr(
		not(target_os = "linux"),
		diagnostic(help("this should not happen on your platform"))
	)]
	TooManyWatches(#[source] notify::Error),

	/// Error received when creating or updating a filesystem watcher fails because there are too many file handles open.
	///
	/// This is the OS error 24 on Linux. It may also occur when the limit for inotify instances is reached.
	#[error("failed to instantiate: too many handles")]
	#[cfg_attr(target_os = "linux", diagnostic(help("you will want to increase your `nofile` limit, see pam_limits(8); or increase your inotify.max_user_instances, see inotify(7) and https://watchexec.github.io/docs/inotify-limits.html")))]
	#[cfg_attr(
		not(target_os = "linux"),
		diagnostic(help("this should not happen on your platform"))
	)]
	TooManyHandles(#[source] notify::Error),

	/// Error received when reading a filesystem event fails.
	#[error("received an event that we could not read")]
	Event(#[source] notify::Error),

	/// Error received when adding to the pathset for the filesystem watcher fails.
	#[error("while adding {path:?}")]
	PathAdd {
		/// The path that was attempted to be added.
		path: PathBuf,

		/// The underlying error.
		#[source]
		err: notify::Error,
	},

	/// Error received when removing from the pathset for the filesystem watcher fails.
	#[error("while removing {path:?}")]
	PathRemove {
		/// The path that was attempted to be removed.
		path: PathBuf,

		/// The underlying error.
		#[source]
		err: notify::Error,
	},
}

/// Errors emitted by the keyboard watcher.
#[derive(Debug, Diagnostic, Error)]
#[non_exhaustive]
pub enum KeyboardWatcherError {
	/// Error received when shutting down stdin watcher fails.
	#[error("failed to shut down stdin watcher")]
	StdinShutdown,
}
