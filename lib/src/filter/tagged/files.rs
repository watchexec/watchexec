//! Load "tagged filter files".

use std::{
	env,
	io::Error,
	path::{Path, PathBuf},
	str::FromStr,
};

use tokio::fs::read_to_string;

use crate::ignore::files::{discover_file, IgnoreFile};

use super::{error::TaggedFiltererError, Filter};

/// A filter file.
///
/// This is merely a type wrapper around an [`IgnoreFile`], as the only difference is how the file
/// is parsed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FilterFile(pub IgnoreFile);

/// Finds all filter files that apply to the current runtime.
///
/// This considers:
/// - `$XDG_CONFIG_HOME/watchexec/filter`, as well as other locations (APPDATA on Windows…)
/// - Files from the `WATCHEXEC_FILTER_FILES` environment variable (comma-separated)
///
/// All errors (permissions, etc) are collected and returned alongside the ignore files: you may
/// want to show them to the user while still using whatever ignores were successfully found. Errors
/// from files not being found are silently ignored (the files are just not returned).
pub async fn from_environment() -> (Vec<FilterFile>, Vec<Error>) {
	let mut files = Vec::new();
	let mut errors = Vec::new();

	for path in env::var("WATCHEXEC_FILTER_FILES")
		.unwrap_or_default()
		.split(',')
	{
		discover_file(&mut files, &mut errors, None, None, PathBuf::from(path)).await;
	}

	let mut wgis = Vec::with_capacity(5);
	if let Ok(home) = env::var("XDG_CONFIG_HOME") {
		wgis.push(Path::new(&home).join("watchexec/filter"));
	}
	if let Ok(home) = env::var("APPDATA") {
		wgis.push(Path::new(&home).join("watchexec/filter"));
	}
	if let Ok(home) = env::var("USERPROFILE") {
		wgis.push(Path::new(&home).join(".watchexec/filter"));
	}
	if let Ok(home) = env::var("HOME") {
		wgis.push(Path::new(&home).join(".watchexec/filter"));
	}

	for path in wgis {
		if discover_file(&mut files, &mut errors, None, None, path).await {
			break;
		}
	}

	(files.into_iter().map(FilterFile).collect(), errors)
}

impl FilterFile {
	/// Read and parse into [`Filter`]s.
	///
	/// Empty lines and lines starting with `#` are ignored. The `applies_in` field of the
	/// [`IgnoreFile`] is used for the `in_path` field of each [`Filter`].
	///
	/// This method reads the entire file into memory.
	pub async fn load(&self) -> Result<Vec<Filter>, TaggedFiltererError> {
		let content = read_to_string(&self.0.path).await?;
		let lines = content.lines();
		let mut filters = Vec::with_capacity(lines.size_hint().0);

		for line in lines {
			if line.is_empty() || line.starts_with('#') {
				continue;
			}

			let mut f = Filter::from_str(line)?;
			f.in_path = self.0.applies_in.clone();
			filters.push(f);
		}

		Ok(filters)
	}
}
