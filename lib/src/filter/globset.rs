//! A simple filterer in the style of the watchexec v1 filter.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use tokio::fs::read_to_string;
use tracing::{debug, trace, trace_span};

use crate::error::RuntimeError;
use crate::event::{Event, FileType};
use crate::filter::Filterer;
use crate::ignore::IgnoreFile;

/// A path-only filterer based on globsets.
///
/// This filterer mimics the behavior of the `watchexec` v1 filter, but does not match it exactly,
/// due to differing internals. It is intended to be used as a stopgap until the tagged filter
/// reaches a stable state or becomes the default. As such it does not have an updatable
/// configuration.
#[derive(Debug)]
pub struct GlobsetFilterer {
	filters: Gitignore,
	ignores: Gitignore,
	extensions: Vec<OsString>,
}

impl GlobsetFilterer {
	/// Create a new `GlobsetFilterer` from a project origin, allowed extensions, and lists of globs.
	///
	/// The first list is used to filter paths (only matching paths will pass the filter), the
	/// second is used to ignore paths (matching paths will fail the pattern). If the filter list is
	/// empty, only the ignore list will be used. If both lists are empty, the filter always passes.
	///
	/// The extensions list is used to filter files by extension.
	///
	/// Non-path events are always passed.
	pub fn new(
		origin: impl AsRef<Path>,
		filters: impl IntoIterator<Item = (String, Option<PathBuf>)>,
		ignores: impl IntoIterator<Item = (String, Option<PathBuf>)>,
		extensions: impl IntoIterator<Item = OsString>,
	) -> Result<Self, ignore::Error> {
		let mut filters_builder = GitignoreBuilder::new(origin);
		let mut ignores_builder = filters_builder.clone();

		for (filter, in_path) in filters {
			trace!(filter=?&filter, "add filter to globset filterer");
			filters_builder.add_line(in_path, &filter)?;
		}

		for (ignore, in_path) in ignores {
			trace!(ignore=?&ignore, "add ignore to globset filterer");
			ignores_builder.add_line(in_path, &ignore)?;
		}

		let filters = filters_builder.build()?;
		let ignores = ignores_builder.build()?;
		let extensions: Vec<OsString> = extensions.into_iter().collect();
		debug!(
			num_filters=%filters.num_ignores(),
			num_neg_filters=%filters.num_whitelists(),
			num_ignores=%ignores.num_ignores(),
			num_neg_ignores=%ignores.num_whitelists(),
			num_extensions=%extensions.len(),
		"globset filterer built");

		Ok(Self {
			filters,
			ignores,
			extensions,
		})
	}

	/// Produces a list of ignore patterns compatible with [`new`][GlobsetFilterer::new()] from an [`IgnoreFile`].
	pub async fn list_from_ignore_file(
		ig: &IgnoreFile,
	) -> Result<Vec<(String, Option<PathBuf>)>, RuntimeError> {
		let content = read_to_string(&ig.path).await?;
		let lines = content.lines();
		let mut ignores = Vec::with_capacity(lines.size_hint().0);

		for line in lines {
			if line.is_empty() || line.starts_with('#') {
				continue;
			}

			ignores.push((line.to_owned(), ig.applies_in.clone()));
		}

		Ok(ignores)
	}
}

impl Filterer for GlobsetFilterer {
	/// Filter an event.
	///
	/// This implementation never errors.
	fn check_event(&self, event: &Event) -> Result<bool, RuntimeError> {
		// TODO: integrate ignore::Filter

		let _span = trace_span!("filterer_check").entered();
		for (path, file_type) in event.paths() {
			let _span = trace_span!("path", ?path).entered();
			let is_dir = file_type
				.map(|t| matches!(t, FileType::Dir))
				.unwrap_or(false);

			if self.ignores.matched(path, is_dir).is_ignore() {
				trace!("ignored by globset ignore");
				return Ok(false);
			}

			if self.filters.num_ignores() > 0 && !self.filters.matched(path, is_dir).is_ignore() {
				trace!("ignored by globset filters");
				return Ok(false);
			}

			if !self.extensions.is_empty() {
				if is_dir {
					trace!("omitted from extension check due to being a dir");
					continue;
				}

				if let Some(ext) = path.extension() {
					if !self.extensions.iter().any(|e| e == ext) {
						trace!("ignored by extension filter");
						return Ok(false);
					}
				} else {
					trace!(
						?path,
						"omitted from extension check due to having no extension"
					);
					continue;
				}
			}
		}

		Ok(true)
	}
}
