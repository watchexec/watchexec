use std::collections::HashMap;

use ignore::gitignore::Gitignore;
use miette::Diagnostic;
use thiserror::Error;
use tokio::sync::watch::error::SendError;

use watchexec::{error::RuntimeError, ignore::IgnoreFilterer};

use crate::{Filter, Matcher};

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
	Ignore(#[source] ignore_files::Error),

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
