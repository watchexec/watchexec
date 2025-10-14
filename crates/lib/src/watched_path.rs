use std::path::{Path, PathBuf};

/// A path to watch.
///
/// Can be a recursive or non-recursive watch.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WatchedPath {
	pub(crate) path: PathBuf,
	pub(crate) recursive: bool,
}

impl From<PathBuf> for WatchedPath {
	fn from(path: PathBuf) -> Self {
		Self {
			path,
			recursive: true,
		}
	}
}

impl From<&str> for WatchedPath {
	fn from(path: &str) -> Self {
		Self {
			path: path.into(),
			recursive: true,
		}
	}
}

impl From<String> for WatchedPath {
	fn from(path: String) -> Self {
		Self {
			path: path.into(),
			recursive: true,
		}
	}
}

impl From<&Path> for WatchedPath {
	fn from(path: &Path) -> Self {
		Self {
			path: path.into(),
			recursive: true,
		}
	}
}

impl From<WatchedPath> for PathBuf {
	fn from(path: WatchedPath) -> Self {
		path.path
	}
}

impl From<&WatchedPath> for PathBuf {
	fn from(path: &WatchedPath) -> Self {
		path.path.clone()
	}
}

impl AsRef<Path> for WatchedPath {
	fn as_ref(&self) -> &Path {
		self.path.as_ref()
	}
}

impl WatchedPath {
	/// Create a new watched path, recursively descending into subdirectories.
	pub fn recursive(path: impl Into<PathBuf>) -> Self {
		Self {
			path: path.into(),
			recursive: true,
		}
	}

	/// Create a new watched path, not descending into subdirectories.
	pub fn non_recursive(path: impl Into<PathBuf>) -> Self {
		Self {
			path: path.into(),
			recursive: false,
		}
	}

	/// Return whether wathching this file will recurse into subdirectories
	pub fn is_recursive(&self) -> bool {
		self.recursive
	}
}
