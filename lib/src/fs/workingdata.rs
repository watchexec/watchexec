use std::{
	collections::HashSet,
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
	pub pathset: HashSet<WatchedPath>,

	/// The kind of watcher to be used.
	pub watcher: Watcher,
}

/// A path to watch, and how to do so.
///
/// Note that this implements `Ord` by ignoring the value of the `recurse`
/// field: only the `dirpath` is considered.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct WatchedPath {
	/// The directory path.
	///
	/// This should not be a file: instead, watch the parent directory, and
	/// filter relevant _events_ (i.e. not with this struct's `filterer`) to
	/// just that file's.
	pub dirpath: PathBuf,

	/// Whether to recurse into subdirectories, and the strategy to use.
	pub recursive: Recurse,
}

impl WatchedPath {
	/// Create a new `WatchedPath` from a path and the default recurse strategy.
	pub fn new(dirpath: impl Into<PathBuf>) -> Self {
		Self {
			dirpath: dirpath.into(),
			recursive: Recurse::default(),
		}
	}

	/// Create a new `WatchedPath` from a path and a recurse strategy.
	pub fn new_with_recurse(dirpath: impl Into<PathBuf>, recursive: Recurse) -> Self {
		Self {
			dirpath: dirpath.into(),
			recursive,
		}
	}

	/// Create a new non-recursive `WatchedPath` from a path.
	pub fn non_recursive(dirpath: impl Into<PathBuf>) -> Self {
		Self {
			dirpath: dirpath.into(),
			recursive: Recurse::No,
		}
	}

	/// Create a new filtered recursive `WatchedPath` from a path and a filterer.
	pub fn filtered(dirpath: impl Into<PathBuf>, filterer: &Arc<dyn Filterer>) -> Self {
		Self {
			dirpath: dirpath.into(),
			recursive: Recurse::Filtered(filterer.clone()),
		}
	}
}

/// The strategy to use when recursing into subdirectories.
///
/// Note that this implements `Eq` and `Hash` by ignoring the value of the
/// `Filtered` variant, so two paths watched with different `Filtered` variants
/// will be considered equal and will replace each other in the pathset; for
/// best results prefer _not_ to do that.
///
/// This is marked non-exhaustive so new strategies can be added without breaking.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Recurse {
	/// Do not recurse.
	No,

	/// Recurse into subdirectories with the native/Notify implementation.
	///
	/// There is no control over how the recursion is done, but it may use
	/// native APIs, which would be more efficient.
	///
	/// This is the default.
	Native,

	/// Recurse into subdirectories with this module's implementation.
	///
	/// Recursion is controlled via a [`Filterer`], which is invoked for every
	/// folder candidate, and should return `true` if the folder is to be
	/// watched (and recursed into).
	///
	/// The default (noop) filterer `()` may be used to recurse into all
	/// subdirectories, but consider using `Native` instead in that case.
	Filtered(Arc<dyn Filterer>),
}

impl Default for Recurse {
	fn default() -> Self {
		Self::Native
	}
}

impl PartialEq for Recurse {
	fn eq(&self, other: &Self) -> bool {
		match (self, other) {
			(Self::Filtered(_), Self::Filtered(_)) => true,
			_ => std::mem::discriminant(self) == std::mem::discriminant(other),
		}
	}
}

impl std::hash::Hash for Recurse {
	fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
		std::mem::discriminant(self).hash(state);
	}
}

impl Eq for Recurse {}

impl PartialOrd for WatchedPath {
	fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
		self.dirpath.partial_cmp(&other.dirpath)
	}
}

impl Ord for WatchedPath {
	fn cmp(&self, other: &Self) -> std::cmp::Ordering {
		self.dirpath.cmp(&other.dirpath)
	}
}

impl From<PathBuf> for WatchedPath {
	fn from(path: PathBuf) -> Self {
		Self::new(path)
	}
}

impl From<&str> for WatchedPath {
	fn from(path: &str) -> Self {
		Self::new(path)
	}
}

impl From<&Path> for WatchedPath {
	fn from(path: &Path) -> Self {
		Self::new(path)
	}
}

impl From<WatchedPath> for PathBuf {
	fn from(path: WatchedPath) -> Self {
		path.dirpath
	}
}

impl AsRef<Path> for WatchedPath {
	fn as_ref(&self) -> &Path {
		self.dirpath.as_ref()
	}
}
