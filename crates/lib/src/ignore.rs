//! A Watchexec Filterer implementation for ignore files.
//!
//! This filterer is meant to be used as a backing filterer inside a more complex or complete
//! filterer, and not as a standalone filterer.

use ignore::Match;
use ignore_files::IgnoreFilter;
use tracing::{trace, trace_span};

use crate::{
	error::RuntimeError,
	event::{Event, FileType, Priority},
	filter::Filterer,
};

/// A Watchexec [`Filterer`] implementation for [`IgnoreFilter`].
#[derive(Clone, Debug)]
pub struct IgnoreFilterer(pub IgnoreFilter);

impl Filterer for IgnoreFilterer {
	/// Filter an event.
	///
	/// This implementation never errors. It returns `Ok(false)` if the event is ignored according
	/// to the ignore files, and `Ok(true)` otherwise. It ignores event priority.
	fn check_event(&self, event: &Event, _priority: Priority) -> Result<bool, RuntimeError> {
		let _span = trace_span!("filterer_check").entered();
		let mut pass = true;

		for (path, file_type) in event.paths() {
			let _span = trace_span!("checking_against_compiled", ?path, ?file_type).entered();
			let is_dir = file_type
				.map(|t| matches!(t, FileType::Dir))
				.unwrap_or(false);

			match self.0.match_path(path, is_dir) {
				Match::None => {
					trace!("no match (pass)");
					pass &= true;
				}
				Match::Ignore(glob) => {
					if glob.from().map_or(true, |f| path.strip_prefix(f).is_ok()) {
						trace!(?glob, "positive match (fail)");
						pass &= false;
					} else {
						trace!(?glob, "positive match, but not in scope (ignore)");
					}
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
