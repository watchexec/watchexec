//! IgnoreTree manages [GitIgnore] instances for a directory tree.
//!
//! A [GitIgnore] is only meant to be used for files that cover a single
//! directory. That makes it annoying to use when you have ignore files
//! in many directories, and some global ignore files too.
//!
//! This module provides a similar interface, but supports loading
//! ignore files directly for any path, or for the global space.

// This module is a candidate for a new crate.
// TODO: do our own parsing? having all these builders around is meh

use std::{
	collections::BTreeMap,
	path::{Path, PathBuf},
};

use ignore::{
	gitignore::{Gitignore, GitignoreBuilder, Glob},
	Error as IgnoreError, Match,
};

/// An interface for many ignore files in a directory tree.
///
/// This is conceptually a tree of [GitIgnore] instances, rooted at the
/// origin, and the matcher methods figure out which one to call on.
#[derive(Debug)]
pub struct IgnoreTree {
	origin: PathBuf,
	ignores: BTreeMap<PathBuf, GitignoreBuilder>,
	compiled: BTreeMap<PathBuf, Gitignore>,
}

impl IgnoreTree {
	/// Create a new IgnoreTree at the given origin.
	///
	/// The origin is the root of the tree which contains ignore files. Adding
	/// ignores that are higher up than the origin will silently discard them,
	/// as they wouldn't have any effect anyway. To add ignores from global
	/// ignore files, use the dedicated method instead of providing their path.
	pub fn new(origin: impl AsRef<Path>) -> Self {
		let origin = origin.as_ref();
		Self {
			origin: origin.to_owned(),
			ignores: BTreeMap::new(),
			compiled: BTreeMap::new(),
		}
	}

	/// Add a line from an ignore file at a particular path.
	///
	/// The `at` path should be to the directory _containing_ the ignore file,
	/// not to the ignore file itself.
	pub fn add_local_line(&mut self, at: impl AsRef<Path>, line: &str) -> Result<(), IgnoreError> {
		todo!()
	}

	/// Add a line from an ignore file for the global ignore space.
	pub fn add_global_line(&mut self, line: &str) -> Result<(), IgnoreError> {
		self.add_local_line("", line)
	}

	/// Returns the total number of ignore globs.
	pub fn num_ignores(&self) -> u64 {
		todo!()
	}

	/// Returns the total number of whitelisted globs.
	pub fn num_whitelists(&self) -> u64 {
		todo!()
	}

	/// Returns whether the given path (file or directory) matched a pattern.
	// TODO: grow our own return type for this
	pub fn matched(&self, path: impl AsRef<Path>, is_dir: bool) -> Match<&Glob> {
		// do the "is in tree, check parents, otherwise don't" dance ourselves
		todo!()
	}
}
