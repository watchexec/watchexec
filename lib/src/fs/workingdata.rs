use std::{
	path::{Path, PathBuf},
	sync::Arc,
};

use crate::filter::Filterer;

use super::Watcher;

/// The configuration of the [fs][self] worker.
///
/// This is marked non-exhaustive so new configuration can be added without breaking.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct WorkingData {
	/// The set of paths to be watched.
	pub pathset: Vec<WatchedPath>,

	/// The kind of watcher to be used.
	pub watcher: Watcher,

	/// The filterer implementation to use when filtering paths to watch.
	///
	/// This is invoked in this context in a special way, as synthetic events
	/// with [`Source::Internal`] and a single path. This is used to filter out
	/// paths that are not to be watched, in the context of a recursive
	/// filesystem watch where we control the descent.
	///
	/// Even though the default [`Filterer`] is a noop, there is no way to check
	/// that the filterer is always a noop, so this is an [`Option`]: if `Some`,
	/// the internal watcher implementation will use non-recursive watching and
	/// this module does the descent itself, otherwise the recursion is left to
	/// the discretion of the Notify library.
	pub filterer: Option<Arc<dyn Filterer>>,
}

/// A path to watch.
///
/// This is currently only a wrapper around a [`PathBuf`], but may be augmented in the future.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WatchedPath(PathBuf);

impl From<PathBuf> for WatchedPath {
	fn from(path: PathBuf) -> Self {
		Self(path)
	}
}

impl From<&str> for WatchedPath {
	fn from(path: &str) -> Self {
		Self(path.into())
	}
}

impl From<&Path> for WatchedPath {
	fn from(path: &Path) -> Self {
		Self(path.into())
	}
}

impl From<WatchedPath> for PathBuf {
	fn from(path: WatchedPath) -> Self {
		path.0
	}
}

impl AsRef<Path> for WatchedPath {
	fn as_ref(&self) -> &Path {
		self.0.as_ref()
	}
}
