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
#![deny(rust_2018_idioms)]

use ignore::Match;
use ignore_files::IgnoreFilter;
use tracing::{trace, trace_span};

use watchexec::{
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
			let path = dunce::simplified(path);
			let _span = trace_span!("checking_against_compiled", ?path, ?file_type).entered();
			let is_dir = file_type.map_or(false, |t| matches!(t, FileType::Dir));

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
