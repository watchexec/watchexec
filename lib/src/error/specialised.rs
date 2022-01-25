use std::collections::HashMap;

use ignore::gitignore::Gitignore;
use miette::Diagnostic;
use thiserror::Error;
use tokio::sync::watch::{self, error::SendError};

use crate::{
	action,
	error::RuntimeError,
	filter::tagged::{Filter, Matcher},
	fs,
	ignore::IgnoreFilterer,
};

/// Errors occurring from reconfigs.
#[derive(Debug, Diagnostic, Error)]
#[non_exhaustive]
#[diagnostic(url(docsrs))]
pub enum ReconfigError {
	/// Error received when the action processor cannot be updated.
	#[error("reconfig: action watch: {0}")]
	#[diagnostic(code(watchexec::reconfig::action_watch))]
	ActionWatch(#[from] watch::error::SendError<action::WorkingData>),

	/// Error received when the fs event source cannot be updated.
	#[error("reconfig: fs watch: {0}")]
	#[diagnostic(code(watchexec::reconfig::fs_watch))]
	FsWatch(#[from] watch::error::SendError<fs::WorkingData>),
}

/// Error when parsing a signal from string.
#[derive(Debug, Diagnostic, Error)]
#[error("invalid signal `{src}`: {err}")]
#[diagnostic(code(watchexec::signal::process::parse), url(docsrs))]
pub struct SignalParseError {
	// The string that was parsed.
	#[source_code]
	src: String,

	// The error that occurred.
	err: String,

	// The span of the source which is in error.
	#[label = "invalid signal"]
	span: (usize, usize),
}

impl SignalParseError {
	pub(crate) fn new(src: &str, err: &str) -> Self {
		Self {
			src: src.to_owned(),
			err: err.to_owned(),
			span: (0, src.len()),
		}
	}
}

/// Errors emitted by the TaggedFilterer.
#[derive(Debug, Diagnostic, Error)]
#[non_exhaustive]
#[diagnostic(url(docsrs))]
pub enum TaggedFiltererError {
	/// Generic I/O error, with some context.
	#[error("io({about}): {err}")]
	#[diagnostic(code(watchexec::filter::io_error))]
	IoError {
		/// What it was about.
		about: &'static str,

		/// The I/O error which occurred.
		#[source]
		err: std::io::Error,
	},

	/// Error received when a tagged filter cannot be parsed.
	#[error("cannot parse filter `{src}`: {err:?}")]
	#[diagnostic(code(watchexec::filter::tagged::parse))]
	Parse {
		/// The source of the filter.
		#[source_code]
		src: String,

		/// What went wrong.
		err: nom::error::ErrorKind,
	},

	/// Error received when a filter cannot be added or removed from a tagged filter list.
	#[error("cannot {action} filter: {err:?}")]
	#[diagnostic(code(watchexec::filter::tagged::filter_change))]
	FilterChange {
		/// The action that was attempted.
		action: &'static str,

		/// The underlying error.
		#[source]
		err: SendError<HashMap<Matcher, Vec<Filter>>>,
	},

	/// Error received when a glob cannot be parsed.
	#[error("cannot parse glob: {0}")]
	#[diagnostic(code(watchexec::filter::tagged::glob_parse))]
	GlobParse(#[source] ignore::Error),

	/// Error received when a compiled globset cannot be changed.
	#[error("cannot change compiled globset: {0:?}")]
	#[diagnostic(code(watchexec::filter::tagged::globset_change))]
	GlobsetChange(#[source] SendError<Option<Gitignore>>),

	/// Error received about the internal ignore filterer.
	#[error("ignore filterer: {0}")]
	#[diagnostic(code(watchexec::filter::tagged::ignore))]
	Ignore(#[source] RuntimeError),

	/// Error received when a new ignore filterer cannot be swapped in.
	#[error("cannot swap in new ignore filterer: {0:?}")]
	#[diagnostic(code(watchexec::filter::tagged::ignore_swap))]
	IgnoreSwap(#[source] SendError<IgnoreFilterer>),
}

impl From<TaggedFiltererError> for RuntimeError {
	fn from(err: TaggedFiltererError) -> Self {
		Self::Filterer {
			kind: "tagged",
			err: Box::new(err) as _,
		}
	}
}
