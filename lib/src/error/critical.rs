use miette::Diagnostic;
use thiserror::Error;
use tokio::{sync::mpsc, task::JoinError};

use crate::event::Event;

use super::RuntimeError;

/// Errors which are not recoverable and stop watchexec execution.
#[derive(Debug, Diagnostic, Error)]
#[non_exhaustive]
#[diagnostic(url(docsrs))]
pub enum CriticalError {
	/// Pseudo-error used to signal a graceful exit.
	#[error("this should never be printed (exit)")]
	#[diagnostic(code(watchexec::runtime::exit))]
	Exit,

	/// For custom critical errors.
	///
	/// This should be used for errors by external code which are not covered by the other error
	/// types; watchexec-internal errors should never use this.
	#[error("external(critical): {0}")]
	#[diagnostic(code(watchexec::critical::external))]
	External(#[from] Box<dyn std::error::Error + Send + Sync>),

	/// A critical I/O error occurred (generic).
	#[error("io(unspecified): {0}")]
	#[diagnostic(code(watchexec::critical::io_error_generic))]
	IoErrorGeneric(#[from] std::io::Error),

	/// A critical I/O error occurred (specific).
	#[error("io({about}): {err}")]
	#[diagnostic(code(watchexec::critical::io_error))]
	IoError {
		/// What it was about.
		about: &'static str,

		/// The I/O error which occurred.
		#[source]
		err: std::io::Error,
	},

	/// Error received when a runtime error cannot be sent to the errors channel.
	#[error("cannot send internal runtime error: {0}")]
	#[diagnostic(code(watchexec::critical::error_channel_send))]
	ErrorChannelSend(#[from] mpsc::error::SendError<RuntimeError>),

	/// Error received when an event cannot be sent to the events channel.
	#[error("cannot send event to internal channel: {0}")]
	#[diagnostic(code(watchexec::critical::event_channel_send))]
	EventChannelSend(#[from] mpsc::error::SendError<Event>),

	/// Error received when joining the main watchexec task.
	#[error("main task join: {0}")]
	#[diagnostic(code(watchexec::critical::main_task_join))]
	MainTaskJoin(#[source] JoinError),

	/// Error received when a handler is missing on initialisation.
	///
	/// This is a **bug** and should be reported.
	#[error("internal: missing handler on init")]
	#[diagnostic(code(watchexec::critical::internal::missing_handler))]
	MissingHandler,
}
