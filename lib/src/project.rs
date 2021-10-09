//! Detect project type and origin.

use std::path::{Path, PathBuf};

use crate::error::CriticalError;

pub async fn origin(path: impl AsRef<Path>) -> Result<PathBuf, CriticalError> {
	todo!()
}

/// Returns all project types detected at this given origin.
///
/// This should be called with the result of [`origin()`], or a project origin if already known; it
/// will not find the origin itself.
pub async fn types(path: impl AsRef<Path>) -> Result<Vec<ProjectType>, CriticalError> {
	todo!()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectType {
	Git,
	Mercurial,
	Pijul,
	Fossil,

	Cargo,
	JavaScript,
	Bundler,
	RubyGem,
	Pip,
}
