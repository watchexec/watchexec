//! Wrapper around an ignore file path.
//!
//! This is a single-type crate that defines the [`IgnoreFile`] type, a wrapper around a path to an
//! ignore file, where it applies, and what project type it is for.

use std::path::PathBuf;

use project_origins::ProjectType;

/// An ignore file.
///
/// This records both the path to the ignore file and some basic metadata about it: which project
/// type it applies to if any, and which subtree it applies in if any (`None` = global ignore file).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IgnoreFile {
	/// The path to the ignore file.
	pub path: PathBuf,

	/// The path to the subtree the ignore file applies to, or `None` for global ignores.
	pub applies_in: Option<PathBuf>,

	/// Which project type the ignore file applies to, or was found through.
	pub applies_to: Option<ProjectType>,
}
