//! A path-only Watchexec filterer based on globsets.
//!
//! This filterer mimics the behavior of the `watchexec` v1 filter, but does not match it exactly,
//! due to differing internals. It is used as the default filterer in Watchexec CLI currently.

#![doc(html_favicon_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![doc(html_logo_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![warn(clippy::unwrap_used, missing_docs)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![deny(rust_2018_idioms)]

use std::{
	collections::HashSet,
	ffi::OsString,
	path::{Path, PathBuf},
};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore_files::{Error, IgnoreFile, IgnoreFilter};
use normalize_path::NormalizePath;
use tracing::{debug, trace, trace_span};
use watchexec::{error::RuntimeError, filter::Filterer};
use watchexec_events::{Event, FileType, Priority, Tag};
use watchexec_filterer_ignore::IgnoreFilterer;

fn simplify_path(path: &Path) -> PathBuf {
	dunce::simplified(path).normalize()
}

/// A simple filterer in the style of the watchexec v1.17 filter.
///
/// Its source-directory check uses ignore files and ignore globs only. Positive filters, extension
/// filters, and the exact-path whitelist remain event-only so they cannot prune directories which
/// may contain matching events.
#[cfg_attr(feature = "full_debug", derive(Debug))]
pub struct GlobsetFilterer {
	#[cfg_attr(not(unix), allow(dead_code))]
	origin: PathBuf,
	filters: Gitignore,
	ignores: Gitignore,
	whitelist: HashSet<PathBuf>,
	ignore_files: IgnoreFilterer,
	extensions: Vec<OsString>,
}

#[cfg(not(feature = "full_debug"))]
impl std::fmt::Debug for GlobsetFilterer {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("GlobsetFilterer")
			.field("origin", &self.origin)
			.field("filters", &"ignore::gitignore::Gitignore{...}")
			.field("ignores", &"ignore::gitignore::Gitignore{...}")
			.field("ignore_files", &self.ignore_files)
			.field("extensions", &self.extensions)
			.finish()
	}
}

impl GlobsetFilterer {
	/// Create a new `GlobsetFilterer` from a project origin, allowed extensions, and lists of globs.
	///
	/// The first list is used to filter paths (only matching paths will pass the filter), the
	/// second is used to ignore paths (matching paths will fail the pattern). If the filter list is
	/// empty, only the ignore list will be used. If both lists are empty, the filter always passes.
	/// Whitelist is used to automatically accept files even if they would be filtered out
	/// otherwise. It is passed as an absolute path to the file that should not be filtered.
	///
	/// Ignores and filters are passed as a tuple of the glob pattern as a string and an optional
	/// path of the folder the pattern should apply in (e.g. the folder a gitignore file is in).
	/// A `None` to the latter will mark the pattern as being global.
	///
	/// The extensions list is used to filter files by extension.
	///
	/// Non-path events are always passed.
	///
	/// Ignore files are read during construction and are not monitored for later edits. Build a new
	/// filterer to load changed or newly discovered files.
	#[allow(clippy::future_not_send)]
	pub async fn new(
		origin: impl AsRef<Path>,
		filters: impl IntoIterator<Item = (String, Option<PathBuf>)>,
		ignores: impl IntoIterator<Item = (String, Option<PathBuf>)>,
		whitelist: impl IntoIterator<Item = PathBuf>,
		ignore_files: impl IntoIterator<Item = IgnoreFile>,
		extensions: impl IntoIterator<Item = OsString>,
	) -> Result<Self, Error> {
		let requested_origin = origin.as_ref();
		let origin = dunce::canonicalize(requested_origin).map_err(|err| Error::Canonicalize {
			path: requested_origin.to_owned(),
			err,
		})?;
		let origin = simplify_path(&origin);
		let mut filters_builder = GitignoreBuilder::new(&origin);
		let mut ignores_builder = GitignoreBuilder::new(&origin);

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

		let ignore_files = ignore_files.into_iter().collect::<Vec<_>>();
		let mut ignore_files = if ignore_files.is_empty() {
			IgnoreFilter::empty(&origin)
		} else {
			IgnoreFilter::new(&origin, &ignore_files).await?
		};
		ignore_files.finish();
		let ignore_files = IgnoreFilterer(ignore_files);

		let whitelist = whitelist
			.into_iter()
			.map(|path| simplify_path(&path))
			.collect::<HashSet<_>>();

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
			origin,
			filters,
			ignores,
			whitelist,
			ignore_files,
			extensions,
		})
	}

	/// Return whether an ignore glob rejects this path or an ancestor directory.
	///
	/// Ancestors are checked from the top down, as they would be during a filesystem walk. Once an
	/// ancestor is ignored, a negation matching only a descendant cannot reopen the pruned subtree.
	/// Paths outside the project origin are matched exactly, since this filterer does not know their
	/// traversal root and must not apply project-relative globs to arbitrary filesystem ancestors.
	fn ignored_by_globs(&self, path: &Path, is_dir: bool) -> bool {
		let ancestors = path
			.strip_prefix(&self.origin)
			.ok()
			.map(|_| {
				path.ancestors()
					.skip(1)
					.take_while(|ancestor| ancestor.starts_with(&self.origin))
					.collect::<Vec<_>>()
			})
			.unwrap_or_default();

		for ancestor in ancestors.into_iter().rev() {
			if self.ignores.matched(ancestor, true).is_ignore() {
				trace!(?path, ?ancestor, "ignored by ancestor globset ignore");
				return true;
			}
		}

		self.ignores.matched(path, is_dir).is_ignore()
	}
}

impl Filterer for GlobsetFilterer {
	/// Filter a source directory using ignore files and ignore globs.
	///
	/// Positive filters, extension filters, and the exact-path whitelist are event-only and do not
	/// affect this check.
	fn check_dir(&self, path: &Path) -> Result<bool, RuntimeError> {
		let path = simplify_path(path);
		let path = path.as_path();
		let _span = trace_span!("filterer_check_dir", ?path).entered();

		trace!("checking internal ignore filterer");
		if !self.ignore_files.check_dir(path)? {
			trace!("internal ignore filterer matched (fail)");
			return Ok(false);
		}

		if self.ignored_by_globs(path, true) {
			trace!("ignored by globset ignore");
			Ok(false)
		} else {
			Ok(true)
		}
	}

	/// Filter an event.
	///
	/// This implementation never errors.
	fn check_event(&self, event: &Event, priority: Priority) -> Result<bool, RuntimeError> {
		let _span = trace_span!("filterer_check").entered();
		let mut event = Event {
			tags: event.tags.clone(),
			metadata: Default::default(),
		};
		for tag in &mut event.tags {
			if let Tag::Path { path, .. } = tag {
				*path = simplify_path(path);
			}
		}
		let event = &event;

		{
			trace!("checking internal whitelist");
			// Ideally check path equality backwards for better perf
			// There could be long matching prefixes so we will exit late
			if !self.whitelist.is_empty()
				&& event.paths().any(|(path, _)| self.whitelist.contains(path))
			{
				trace!("internal whitelist filterer matched (success)");
				return Ok(true);
			}
		}

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
				let is_dir = file_type.map_or(false, |t| matches!(t, FileType::Dir));

				if self.ignored_by_globs(path, is_dir) {
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
