use std::path::Path;

use notify::{RecursiveMode, Watcher as _};

use crate::error::FsWatcherError;

use super::Watcher;

/// The backend seam used by the recursor. Managed recursion must only pass
/// `NonRecursive` to this interface.
pub(super) trait Backend: Send {
	fn watch(&mut self, path: &Path, mode: RecursiveMode) -> notify::Result<()>;
	fn unwatch(&mut self, path: &Path) -> notify::Result<()>;
}

/// Temporary backend used only while the worker synchronously recreates a
/// failed watcher. No recursor step is run while this is installed.
pub(super) struct DisconnectedBackend;

impl Backend for DisconnectedBackend {
	fn watch(&mut self, _path: &Path, _mode: RecursiveMode) -> notify::Result<()> {
		Err(notify::Error::generic("filesystem backend is disconnected"))
	}

	fn unwatch(&mut self, _path: &Path) -> notify::Result<()> {
		Err(notify::Error::generic("filesystem backend is disconnected"))
	}
}

impl<T> Backend for T
where
	T: notify::Watcher + Send,
{
	fn watch(&mut self, path: &Path, mode: RecursiveMode) -> notify::Result<()> {
		notify::Watcher::watch(self, path, mode)
	}

	fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
		notify::Watcher::unwatch(self, path)
	}
}

pub(super) fn create_notify(
	kind: Watcher,
	follow_symlinks: bool,
	handler: impl notify::EventHandler,
) -> notify::Result<Box<dyn Backend>> {
	create_notify_for_recommended(
		kind,
		<notify::RecommendedWatcher as notify::Watcher>::kind(),
		follow_symlinks,
		handler,
	)
	.map(|(backend, _)| backend)
}

fn create_notify_for_recommended(
	kind: Watcher,
	recommended: notify::WatcherKind,
	follow_symlinks: bool,
	handler: impl notify::EventHandler,
) -> notify::Result<(Box<dyn Backend>, notify::WatcherKind)> {
	let config = notify_config(kind, follow_symlinks);
	if kind.backend_kind_for(recommended) == notify::WatcherKind::PollWatcher {
		notify::PollWatcher::new(handler, config).map(|watcher| {
			(
				Box::new(watcher) as Box<dyn Backend>,
				<notify::PollWatcher as notify::Watcher>::kind(),
			)
		})
	} else {
		notify::RecommendedWatcher::new(handler, config).map(|watcher| {
			(
				Box::new(watcher) as Box<dyn Backend>,
				<notify::RecommendedWatcher as notify::Watcher>::kind(),
			)
		})
	}
}

fn notify_config(kind: Watcher, follow_symlinks: bool) -> notify::Config {
	let config = notify::Config::default().with_follow_symlinks(follow_symlinks);
	match kind {
		Watcher::Native => config,
		Watcher::Poll(delay) => config.with_poll_interval(delay),
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ResourceError {
	Watches,
	Handles,
}

impl ResourceError {
	pub(super) const fn into_fs_error(self, error: notify::Error) -> FsWatcherError {
		match self {
			Self::Watches => FsWatcherError::TooManyWatches(error),
			Self::Handles => FsWatcherError::TooManyHandles(error),
		}
	}
}

pub(super) fn classify_resource_error(error: &notify::Error) -> Option<ResourceError> {
	if matches!(error.kind, notify::ErrorKind::MaxFilesWatch)
		|| (cfg!(any(target_os = "linux", target_os = "android"))
			&& matches!(error.kind, notify::ErrorKind::Io(ref error) if error.raw_os_error() == Some(28)))
	{
		Some(ResourceError::Watches)
	} else if (cfg!(unix)
		&& matches!(error.kind, notify::ErrorKind::Io(ref error) if matches!(error.raw_os_error(), Some(23 | 24))))
		|| (cfg!(windows)
			&& matches!(error.kind, notify::ErrorKind::Io(ref error) if error.raw_os_error() == Some(4)))
	{
		Some(ResourceError::Handles)
	} else {
		None
	}
}

#[cfg(test)]
mod tests {
	#[cfg(any(unix, windows))]
	use std::io;

	use super::*;

	fn ignore_events(_: notify::Result<notify::Event>) {}

	#[test]
	fn factory_constructs_poll_for_kqueue_recommendation() {
		let (_, actual) = create_notify_for_recommended(
			Watcher::Native,
			notify::WatcherKind::Kqueue,
			true,
			ignore_events,
		)
		.unwrap();
		assert_eq!(actual, notify::WatcherKind::PollWatcher);
	}

	#[test]
	fn factory_constructs_selected_native_backend() {
		let (_, actual) = create_notify_for_recommended(
			Watcher::Native,
			<notify::RecommendedWatcher as notify::Watcher>::kind(),
			true,
			ignore_events,
		)
		.unwrap();
		assert_eq!(actual, Watcher::Native.backend_kind());
	}

	#[test]
	fn factory_constructs_poll_for_explicit_poll() {
		let (_, actual) = create_notify_for_recommended(
			Watcher::Poll(std::time::Duration::from_millis(1234)),
			<notify::RecommendedWatcher as notify::Watcher>::kind(),
			true,
			ignore_events,
		)
		.unwrap();
		assert_eq!(actual, notify::WatcherKind::PollWatcher);
	}

	#[test]
	fn native_poll_uses_notify_default_interval() {
		assert_eq!(
			notify_config(Watcher::Native, true).poll_interval(),
			notify::Config::default().poll_interval()
		);
	}

	#[test]
	fn explicit_poll_uses_configured_interval() {
		let interval = std::time::Duration::from_millis(1234);
		assert_eq!(
			notify_config(Watcher::Poll(interval), true).poll_interval(),
			Some(interval)
		);
	}

	#[test]
	fn notify_config_preserves_symlink_policy() {
		assert!(notify_config(Watcher::Native, true).follow_symlinks());
		assert!(!notify_config(Watcher::Native, false).follow_symlinks());
	}

	#[test]
	fn max_files_watch_is_always_a_resource_error() {
		let error = notify::Error::new(notify::ErrorKind::MaxFilesWatch);
		assert_eq!(
			classify_resource_error(&error),
			Some(ResourceError::Watches)
		);
	}

	#[cfg(any(target_os = "linux", target_os = "android"))]
	#[test]
	fn linux_enospc_is_too_many_watches() {
		let watches = notify::Error::io(io::Error::from_raw_os_error(28));
		assert_eq!(
			classify_resource_error(&watches),
			Some(ResourceError::Watches)
		);
	}

	#[cfg(unix)]
	#[test]
	fn unix_handle_errno_values_are_classified() {
		let process_handles = notify::Error::io(io::Error::from_raw_os_error(24));
		let system_handles = notify::Error::io(io::Error::from_raw_os_error(23));
		assert_eq!(
			classify_resource_error(&process_handles),
			Some(ResourceError::Handles)
		);
		assert_eq!(
			classify_resource_error(&system_handles),
			Some(ResourceError::Handles)
		);
	}

	#[cfg(windows)]
	#[test]
	fn windows_error_four_is_too_many_handles() {
		let error = notify::Error::io(io::Error::from_raw_os_error(4));
		assert_eq!(
			classify_resource_error(&error),
			Some(ResourceError::Handles)
		);
	}
}
