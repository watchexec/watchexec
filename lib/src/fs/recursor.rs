use std::{
	collections::{HashMap, HashSet},
	mem::take,
	path::PathBuf,
};

use notify::{Error as NotifyError, RecursiveMode, Watcher};
use tracing::error;

use crate::error::RuntimeError;

use super::{Recurse, WatchedPath, Watcher as WatcherKind};

// TODO: for Recurse::Native, unwatch and re-watch manually while ignoring all the erroring paths
// See https://github.com/watchexec/watchexec/issues/218

#[derive(Debug, Default)]
pub struct PathSet {
	/// Just the recurse:no and recurse:native paths.
	plain: HashSet<WatchedPath>,

	/// The recurse:filtered paths, mapped to the subpaths they're currently watching.
	filtered: HashMap<WatchedPath, HashSet<PathBuf>>,
}

impl PathSet {
	pub(super) fn drain(&mut self) {
		take(&mut self.plain);
		take(&mut self.filtered);
	}

	pub(super) fn len(&self) -> usize {
		self.plain.len().saturating_add(self.filtered.len())
	}

	pub(super) fn contains(&self, path: &WatchedPath) -> bool {
		self.plain.contains(path) || self.filtered.contains_key(path)
	}

	pub(super) fn iter(&self) -> impl Iterator<Item = &WatchedPath> {
		self.plain.iter().chain(self.filtered.keys())
	}

	fn add_plain(&mut self, path: &WatchedPath) {
		self.plain.insert(path.clone());
	}

	fn rm_plain(&mut self, path: &WatchedPath) {
		self.plain.remove(path);
	}
}

impl WatchedPath {
	pub(super) fn watch(
		&self,
		kind: WatcherKind,
		watcher: &mut Box<dyn Watcher + Send>,
		pathset: &mut PathSet,
	) -> Result<(), RuntimeError> {
		match &self.recursive {
			Recurse::No => {
				watcher
					.watch(&self.dirpath, RecursiveMode::NonRecursive)
					.map_err(|err| self.multi_path_error(kind, err, false))?;
				pathset.add_plain(self);
			}
			Recurse::Native => {
				watcher
					.watch(&self.dirpath, RecursiveMode::Recursive)
					.map_err(|err| self.multi_path_error(kind, err, false))?;
				pathset.add_plain(self);
			}
			Recurse::Filtered(_f) => {
				todo!()
			}
		}

		Ok(())
	}

	pub(super) fn unwatch(
		&self,
		kind: WatcherKind,
		watcher: &mut Box<dyn Watcher + Send>,
		pathset: &mut PathSet,
	) -> Result<(), RuntimeError> {
		match &self.recursive {
			Recurse::No | Recurse::Native => {
				watcher
					.unwatch(&self.dirpath)
					.map_err(|err| self.multi_path_error(kind, err, false))?;
				pathset.rm_plain(self);
			}
			Recurse::Filtered(_f) => {
				todo!()
			}
		}

		Ok(())
	}

	fn multi_path_error(&self, kind: WatcherKind, mut err: NotifyError, rm: bool) -> RuntimeError {
		error!(?err, "notify {}watch() error", if rm { "un" } else { "" });

		let mut paths = take(&mut err.paths);
		if paths.is_empty() {
			paths.push(self.dirpath.clone());
		}

		let generic = err.to_string();
		let mut err = Some(err);

		let mut errs = Vec::with_capacity(paths.len());
		for path in paths {
			let e = err
				.take()
				.unwrap_or_else(|| notify::Error::generic(&generic))
				.add_path(path.clone());

			errs.push(if rm {
				RuntimeError::FsWatcherPathRemove { path, kind, err: e }
			} else {
				RuntimeError::FsWatcherPathAdd { path, kind, err: e }
			});
		}

		if errs.len() == 1 {
			errs.pop().expect("corrupt state (just checked)")
		} else {
			RuntimeError::Set(errs)
		}
	}
}
