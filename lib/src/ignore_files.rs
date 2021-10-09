//! Find ignore files, like `.gitignore`, `.ignore`, and others.

use std::path::{Path, PathBuf};

use crate::{error::RuntimeError, project::ProjectType};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IgnoreFile {
	pub path: PathBuf,
	pub applies_in: PathBuf,
	pub applies_to: Option<ProjectType>,
}

/// Finds all ignore files in the given directory and subdirectories.
///
/// This considers:
/// - Git ignore files (`.gitignore`)
/// - Mercurial ignore files (`.hgignore`)
/// - Tool-generic `.ignore` files
/// - `.git/info/exclude` files in the `path` directory only
/// - Git configurable project ignore files (with `core.excludesFile` in `.git/config`)
///
/// Importantly, this should be called from the origin of the project, not a subfolder. This
/// function will not discover the project origin, and will not traverse parent directories. Use the
/// [`project::origin`](crate::project::origin) function for that.
///
/// This function also does not distinguish between project folder types, and collects all files for
/// all supported VCSs and other project types. Use the `applies_to` field to filter the results.
pub async fn from_origin(path: impl AsRef<Path>) -> Result<Vec<IgnoreFile>, RuntimeError> {
	todo!()
}

/// Finds all ignore files that apply to the current runtime.
///
/// This considers:
/// - System-wide ignore files (e.g. `/etc/git/ignore`)
/// - User-specific ignore files (e.g. `~/.gitignore`)
/// - Git configurable ignore files (e.g. with `core.excludesFile` in system or user config)
/// - Other VCS ignore files in system and user locations and config (e.g. `~/.hgignore`)
/// - Files from the `WATCHEXEC_IGNORE_FILES` environment variable (comma-separated).
pub async fn from_environment() -> Result<Vec<IgnoreFile>, RuntimeError> {
	todo!()
}
