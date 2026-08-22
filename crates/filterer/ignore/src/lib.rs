//! A Watchexec Filterer implementation for ignore files.
//!
//! This filterer is meant to be used as a backing filterer inside a more complex or complete
//! filterer, and not as a standalone filterer.
//!
//! This is a fairly simple wrapper around the [`ignore_files`] crate, which is probably where you
//! want to look for any detail or to use this outside of Watchexec.

#![doc(html_favicon_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![doc(html_logo_url = "https://watchexec.github.io/logo:watchexec.svg")]
#![warn(clippy::unwrap_used, missing_docs)]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![deny(rust_2018_idioms)]

use std::path::Path;

use ignore::Match;
use ignore_files::IgnoreFilter;
use tracing::{trace, trace_span};
use watchexec::{error::RuntimeError, filter::Filterer};
use watchexec_events::{Event, FileType, Priority};

/// A Watchexec [`Filterer`] implementation for [`IgnoreFilter`].
///
/// It applies the same top-down ignore semantics to source directories and events. Ignore files are
/// not monitored or rediscovered by this wrapper; update or replace the inner filter explicitly.
#[derive(Clone, Debug)]
pub struct IgnoreFilterer(pub IgnoreFilter);

impl Filterer for IgnoreFilterer {
	/// Filter a source directory.
	///
	/// This implementation never errors. It returns `Ok(false)` if the directory is ignored
	/// according to the ignore files, and `Ok(true)` otherwise.
	fn check_dir(&self, path: &Path) -> Result<bool, RuntimeError> {
		Ok(self.0.check_dir(path))
	}

	/// Filter an event.
	///
	/// This implementation never errors. It returns `Ok(false)` if the event is ignored according
	/// to the ignore files, and `Ok(true)` otherwise. It ignores event priority.
	fn check_event(&self, event: &Event, _priority: Priority) -> Result<bool, RuntimeError> {
		let _span = trace_span!("filterer_check").entered();
		let mut pass = true;

		for (path, file_type) in event.paths() {
			let _span = trace_span!("checking_against_compiled", ?path, ?file_type).entered();
			let is_dir = file_type.map_or(false, |t| matches!(t, FileType::Dir));

			match self.0.match_path_or_ancestors(path, is_dir) {
				Match::None => trace!("no match (pass)"),
				Match::Ignore(glob) => {
					trace!(?glob, "positive match (fail)");
					pass = false;
				}
				Match::Whitelist(glob) => {
					trace!(?glob, "negative match (pass)");
					pass = true;
				}
			}
		}

		trace!(?pass, "verdict");
		Ok(pass)
	}
}
