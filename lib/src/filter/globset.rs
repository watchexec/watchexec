//! A simple filterer in the style of the watchexec v1 filter.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use tracing::{debug, trace, trace_span};

use crate::error::RuntimeError;
use crate::event::{Event, FileType};
use crate::filter::Filterer;
use crate::ignore::{IgnoreFile, IgnoreFilterer};

/// A path-only filterer based on globsets.
///
/// This filterer mimics the behavior of the `watchexec` v1 filter, but does not match it exactly,
/// due to differing internals. It is intended to be used as a stopgap until the tagged filterer
/// or another advanced filterer, reaches a stable state or becomes the default. As such it does not
/// have an updatable configuration.
#[derive(Debug)]
pub struct GlobsetFilterer {
	filters: Gitignore,
	ignores: Gitignore,
	ignore_files: IgnoreFilterer,
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
	pub async fn new(
		origin: impl AsRef<Path>,
		filters: impl IntoIterator<Item = (String, Option<PathBuf>)>,
		ignores: impl IntoIterator<Item = (String, Option<PathBuf>)>,
		ignore_files: impl IntoIterator<Item = IgnoreFile>,
		extensions: impl IntoIterator<Item = OsString>,
	) -> Result<Self, RuntimeError> {
		let origin = origin.as_ref();
		let mut filters_builder = GitignoreBuilder::new(&origin);
		let mut ignores_builder = GitignoreBuilder::new(&origin);

		for (filter, in_path) in filters {
			trace!(filter=?&filter, "add filter to globset filterer");
			filters_builder
				.add_line(in_path.clone(), &filter)
				.map_err(|err| RuntimeError::GlobsetGlob { file: in_path, err })?;
		}

		for (ignore, in_path) in ignores {
			trace!(ignore=?&ignore, "add ignore to globset filterer");
			ignores_builder
				.add_line(in_path.clone(), &ignore)
				.map_err(|err| RuntimeError::GlobsetGlob { file: in_path, err })?;
		}

		let filters = filters_builder
			.build()
			.map_err(|err| RuntimeError::GlobsetGlob { file: None, err })?;
		let ignores = ignores_builder
			.build()
			.map_err(|err| RuntimeError::GlobsetGlob { file: None, err })?;

		let extensions: Vec<OsString> = extensions.into_iter().collect();

		let mut ignore_files =
			IgnoreFilterer::new(origin, &ignore_files.into_iter().collect::<Vec<_>>()).await?;
		ignore_files.finish();

		debug!(
			num_filters=%filters.num_ignores(),
			num_neg_filters=%filters.num_whitelists(),
			num_ignores=%ignores.num_ignores(),
			num_in_ignore_files=?ignore_files.num_ignores(),
			num_neg_ignores=%ignores.num_whitelists(),
			num_extensions=%extensions.len(),
		"globset filterer built");

		Ok(Self {
			filters,
			ignores,
			ignore_files,
			extensions,
		})
	}
}

impl Filterer for GlobsetFilterer {
	/// Filter an event.
	///
	/// This implementation never errors.
	fn check_event(&self, event: &Event) -> Result<bool, RuntimeError> {
		let _span = trace_span!("filterer_check").entered();

		{
			trace!("checking internal ignore filterer");
			if !self
				.ignore_files
				.check_event(event)
				.expect("IgnoreFilterer never errors")
			{
				trace!("internal ignore filterer matched (fail)");
				return Ok(false);
			}
		}
		
		let mut paths = event.paths().peekable();
		if paths.peek().is_none() {
			trace!("non-path event (pass)");
			Ok(true)
		} else {
			Ok(paths.any(|(path, file_type)| {
				let _span = trace_span!("path", ?path).entered();
				let is_dir = file_type
					.map(|t| matches!(t, FileType::Dir))
					.unwrap_or(false);

				if self.ignores.matched(path, is_dir).is_ignore() {
					trace!("ignored by globset ignore");
					return false;
				}

				if self.filters.num_ignores() > 0 && !self.filters.matched(path, is_dir).is_ignore() {
					trace!("ignored by globset filters");
					return false;
				}

				if !self.extensions.is_empty() {
					if is_dir {
						trace!("failed on extension check due to being a dir");
						return false;
					}

					if let Some(ext) = path.extension() {
						if !self.extensions.iter().any(|e| e == ext) {
							trace!("ignored by extension filter");
							return false;
						}
					} else {
						trace!(
							?path,
							"failed on extension check due to having no extension"
						);
						return false;
					}
				}

				true
			}))
		}
	}
}
