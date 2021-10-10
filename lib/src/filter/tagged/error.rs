//! Error type for TaggedFilterer.

use std::collections::HashMap;

use miette::Diagnostic;
use thiserror::Error;
use tokio::sync::watch::error::SendError;

use crate::{
	error::RuntimeError,
	filter::tagged::{Filter, Matcher},
};

/// Errors emitted by the TaggedFilterer.
#[derive(Debug, Diagnostic, Error)]
#[non_exhaustive]
pub enum TaggedFiltererError {
	/// Generic I/O error, with no additional context.
	#[error(transparent)]
	#[diagnostic(code(watchexec::filter::tagged::io_error))]
	IoError(#[from] std::io::Error),

	/// Error received when a tagged filter cannot be parsed.
	#[error("cannot parse filter `{src}`: {err:?}")]
	#[diagnostic(code(watchexec::filter::tagged::parse))]
	Parse {
		src: String,
		err: nom::error::ErrorKind,
		// TODO: use miette's source snippet feature
	},

	/// Error received when a filter cannot be added or removed from a tagged filter list.
	#[error("cannot {action} filter: {err:?}")]
	#[diagnostic(code(watchexec::filter::tagged::change))]
	FilterChange {
		action: &'static str,
		#[source]
		err: SendError<HashMap<Matcher, Vec<Filter>>>,
	},

	/// Error received when a glob cannot be parsed.
	#[error("cannot parse glob: {0}")]
	#[diagnostic(code(watchexec::filter::tagged::glob_parse))]
	GlobParse(#[from] globset::Error),
}

impl From<TaggedFiltererError> for RuntimeError {
	fn from(err: TaggedFiltererError) -> Self {
		Self::Filterer {
			kind: "tagged",
			err: Box::new(err) as _,
		}
	}
}
