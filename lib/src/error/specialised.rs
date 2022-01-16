use miette::Diagnostic;
use thiserror::Error;
use tokio::sync::watch;

use crate::{action, fs};

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

/// Error when parsing a glob pattern from string.
#[derive(Debug, Diagnostic, Error)]
#[error("invalid glob `{src}`: {err}")]
#[diagnostic(code(watchexec::filter::glob_parse), url(docsrs))]
pub struct GlobParseError {
	// The string that was parsed.
	#[source_code]
	src: String,

	// The error that occurred.
	err: String,

	// The span of the source which is in error.
	#[label = "invalid"]
	span: (usize, usize),
}

impl GlobParseError {
	pub(crate) fn new(src: &str, err: &str) -> Self {
		Self {
			src: src.to_owned(),
			err: err.to_owned(),
			span: (0, src.len()),
		}
	}
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
