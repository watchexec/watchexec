//! A path-only Watchexec filterer based on globsets.
//!
//! This filterer mimics the behavior of the `watchexec` v1 filter, but does not match it exactly,
//! due to differing internals. It is used as the default filterer in Watchexec CLI currently.

#![doc(html_favicon_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![doc(html_logo_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![warn(clippy::unwrap_used, missing_docs)]
#![deny(rust_2018_idioms)]

use std::{
	ffi::OsString,
	path::{Path, PathBuf},
};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore_files::{Error, IgnoreFile, IgnoreFilter};
use tracing::{debug, trace, trace_span};
use watchexec::{
	error::RuntimeError,
	event::{Event, FileType, Priority},
	filter::Filterer,
};
use watchexec_filterer_ignore::IgnoreFilterer;

/// A simple filterer in the style of the watchexec v1.17 filter.
#[derive(Debug)]
pub struct GlobsetFilterer {
	#[cfg_attr(not(unix), allow(dead_code))]
	origin: PathBuf,
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
	/// Ignores and filters are passed as a tuple of the glob pattern as a string and an optional
	/// path of the folder the pattern should apply in (e.g. the folder a gitignore file is in).
	/// A `None` to the latter will mark the pattern as being global.
	///
	/// The extensions list is used to filter files by extension.
	///
	/// Non-path events are always passed.
	#[allow(clippy::future_not_send)]
	pub async fn new(
		origin: impl AsRef<Path>,
		filters: impl IntoIterator<Item = (String, Option<PathBuf>)>,
		ignores: impl IntoIterator<Item = (String, Option<PathBuf>)>,
		ignore_files: impl IntoIterator<Item = IgnoreFile>,
		extensions: impl IntoIterator<Item = OsString>,
	) -> Result<Self, Error> {
		let origin = origin.as_ref();
		let mut filters_builder = GitignoreBuilder::new(origin);
		let mut ignores_builder = GitignoreBuilder::new(origin);

		for (filter, in_path) in filters {
			trace!(filter=?&filter, "add filter to globset filterer");
			filters_builder
				.add_line(in_path.clone(), &filter)
				.map_err(|err| Error::Glob { file: in_path, err })?;
		}

		for (ignore, in_path) in ignores {
			trace!(ignore=?&ignore, "add ignore to globset filterer");
			ignores_builder
				.add_line(in_path.clone(), &ignore)
				.map_err(|err| Error::Glob { file: in_path, err })?;
		}

		let filters = filters_builder
			.build()
			.map_err(|err| Error::Glob { file: None, err })?;
		let ignores = ignores_builder
			.build()
			.map_err(|err| Error::Glob { file: None, err })?;

		let extensions: Vec<OsString> = extensions.into_iter().collect();

		let mut ignore_files =
			IgnoreFilter::new(origin, &ignore_files.into_iter().collect::<Vec<_>>()).await?;
		ignore_files.finish();
		let ignore_files = IgnoreFilterer(ignore_files);

		debug!(
			?origin,
			num_filters=%filters.num_ignores(),
			num_neg_filters=%filters.num_whitelists(),
			num_ignores=%ignores.num_ignores(),
			num_in_ignore_files=?ignore_files.0.num_ignores(),
			num_neg_ignores=%ignores.num_whitelists(),
			num_extensions=%extensions.len(),
		"globset filterer built");

		Ok(Self {
			origin: origin.into(),
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
	fn check_event(&self, event: &Event, priority: Priority) -> Result<bool, RuntimeError> {
		let _span = trace_span!("filterer_check").entered();

		{
			trace!("checking internal ignore filterer");
			if !self
				.ignore_files
				.check_event(event, priority)
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
					.map_or(false, |t| matches!(t, FileType::Dir));

				if self.ignores.matched(path, is_dir).is_ignore() {
					trace!("ignored by globset ignore");
					return false;
				}

				let mut filtered = false;
				if self.filters.num_ignores() > 0 {
					trace!("running through glob filters");
					filtered = true;

					if self.filters.matched(path, is_dir).is_ignore() {
						trace!("allowed by globset filters");
						return true;
					}

					// Watchexec 1.x bug, TODO remove at 2.0
					#[cfg(unix)]
					if let Ok(based) = path.strip_prefix(&self.origin) {
						let rebased = {
							use std::path::MAIN_SEPARATOR;
							let mut b = self.origin.clone().into_os_string();
							b.push(PathBuf::from(String::from(MAIN_SEPARATOR)));
							b.push(PathBuf::from(String::from(MAIN_SEPARATOR)));
							b.push(based.as_os_str());
							b
						};

						trace!(?rebased, "testing on rebased path, 1.x bug compat (#258)");
						if self.filters.matched(rebased, is_dir).is_ignore() {
							trace!("allowed by globset filters, 1.x bug compat (#258)");
							return true;
						}
					}
				}

				if !self.extensions.is_empty() {
					trace!("running through extension filters");
					filtered = true;

					if is_dir {
						trace!("failed on extension check due to being a dir");
						return false;
					}

					if let Some(ext) = path.extension() {
						if self.extensions.iter().any(|e| e == ext) {
							trace!("allowed by extension filter");
							return true;
						}
					} else {
						trace!(
							?path,
							"failed on extension check due to having no extension"
						);
						return false;
					}
				}

				!filtered
			}))
		}
	}
}
