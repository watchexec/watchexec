//! A simple filterer in the style of the watchexec v1 filter.

use std::path::Path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use tracing::{debug, trace};

use crate::error::RuntimeError;
use crate::event::Event;
use crate::filter::Filterer;

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
}

impl GlobsetFilterer {
	/// Create a new `GlobsetFilterer` from a project origin and two lists of glob patterns.
	///
	/// The first list is used to filter paths (only matching paths will pass the filter), the
	/// second is used to ignore paths (matching paths will fail the pattern). If the filter list is
	/// empty, only the ignore list will be used. If both lists are empty, the filter always passes.
	///
	/// Non-path events are always passed.
	pub fn new<FI, F, II, P>(
		origin: impl AsRef<Path>,
		filters: FI,
		ignores: II,
	) -> Result<Self, ignore::Error>
	where
		FI: IntoIterator<Item = (F, Option<P>)>,
		F: AsRef<str>,
		II: IntoIterator<Item = (F, Option<P>)>,
		P: AsRef<Path>,
	{
		let mut filters_builder = GitignoreBuilder::new(origin);
		let mut ignores_builder = filters_builder.clone();

		for (filter, in_path) in filters {
			let filter = filter.as_ref();
			trace!(filter, "add filter to globset filterer");
			filters_builder.add_line(in_path.map(|p| p.as_ref().to_owned()), filter)?;
		}

		for (ignore, in_path) in ignores {
			let ignore = ignore.as_ref();
			trace!(ignore, "add ignore to globset filterer");
			ignores_builder.add_line(in_path.map(|p| p.as_ref().to_owned()), ignore)?;
		}

		let filters = filters_builder.build()?;
		let ignores = ignores_builder.build()?;
		debug!(
			num_filters=%filters.num_ignores(),
			num_neg_filters=%filters.num_whitelists(),
			num_ignores=%ignores.num_ignores(),
			num_neg_ignores=%ignores.num_whitelists(),
		"globset filterer built");

		Ok(Self { filters, ignores })
	}
}

impl Filterer for GlobsetFilterer {
	fn check_event(&self, event: &Event) -> Result<bool, RuntimeError> {
		for (path, file_type) in event.paths() {
			let is_dir = file_type.map(|t| t.is_dir()).unwrap_or(false);

			if self.ignores.matched(path, is_dir).is_ignore() {
				trace!(?path, "ignored by globset ignore");
				return Ok(false);
			}

			if self.filters.num_ignores() > 0 && !self.filters.matched(path, is_dir).is_ignore() {
				trace!(?path, "ignored by globset filters");
				return Ok(false);
			}
		}

		Ok(true)
	}
}
