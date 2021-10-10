//! Detect project type and origin.

use std::{
	io::Error,
	path::{Path, PathBuf},
};

pub async fn origins(path: impl AsRef<Path>) -> Result<Vec<PathBuf>, Error> {
	todo!()
}

/// Returns all project types detected at this given origin.
///
/// This should be called with a result of [`origins()`], or a project origin if already known; it
/// will not find the origin itself.
pub async fn types(path: impl AsRef<Path>) -> Result<Vec<ProjectType>, Error> {
	todo!()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ProjectType {
	Bazaar,
	Darcs,
	Fossil,
	Git,
	Mercurial,
	Pijul,

	Bundler,
	Cargo,
	JavaScript,
	Pip,
	RubyGem,
}
