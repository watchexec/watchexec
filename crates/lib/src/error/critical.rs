use miette::Diagnostic;
use thiserror::Error;
use tokio::{sync::mpsc, task::JoinError};
use watchexec_events::{Event, Priority};

use super::{FsWatcherError, RuntimeError};
use crate::fs::Watcher;

/// Errors which are not recoverable and stop watchexec execution.
#[derive(Debug, Diagnostic, Error)]
#[non_exhaustive]
pub enum CriticalError {
	/// Pseudo-error used to signal a graceful exit.
	#[error("this should never be printed (exit)")]
	Exit,

	/// For custom critical errors.
	///
	/// This should be used for errors by external code which are not covered by the other error
	/// types; watchexec-internal errors should never use this.
	#[error("external(critical): {0}")]
	External(#[from] Box<dyn std::error::Error + Send + Sync>),

	/// For elevated runtime errors.
	///
	/// This is used for runtime errors elevated to critical.
	#[error("a runtime error is too serious for the process to continue")]
	Elevated {
		/// The runtime error to be elevated.
		#[source]
		err: RuntimeError,

		/// Some context or help for the user.
		help: Option<String>,
	},

	/// A critical I/O error occurred.
	#[error("io({about}): {err}")]
	IoError {
		/// What it was about.
		about: &'static str,

		/// The I/O error which occurred.
		#[source]
		err: std::io::Error,
	},

	/// Error received when a runtime error cannot be sent to the errors channel.
	#[error("cannot send internal runtime error: {0}")]
	ErrorChannelSend(#[from] mpsc::error::SendError<RuntimeError>),

	/// Error received when an event cannot be sent to the events channel.
	#[error("cannot send event to internal channel: {0}")]
	EventChannelSend(#[from] async_priority_channel::SendError<(Event, Priority)>),

	/// Error received when joining the main watchexec task.
	#[error("main task join: {0}")]
	MainTaskJoin(#[source] JoinError),

	/// Error received when the filesystem watcher can't initialise.
	///
	/// In theory this is recoverable but in practice it's generally not, so we treat it as critical.
	#[error("fs: cannot initialise {kind:?} watcher")]
	FsWatcherInit {
		/// The kind of watcher.
		kind: Watcher,

		/// The error which occurred.
		#[source]
		err: FsWatcherError,
	},
}
