use std::path::PathBuf;

use miette::Diagnostic;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::{event::Event, fs::Watcher, signal::process::SubSignal};

/// Errors which _may_ be recoverable, transient, or only affect a part of the operation, and should
/// be reported to the user and/or acted upon programatically, but will not outright stop watchexec.
#[derive(Debug, Diagnostic, Error)]
#[non_exhaustive]
#[diagnostic(url(docsrs))]
pub enum RuntimeError {
	/// Pseudo-error used to signal a graceful exit.
	#[error("this should never be printed (exit)")]
	#[diagnostic(code(watchexec::runtime::exit))]
	Exit,

	/// For custom runtime errors.
	///
	/// This should be used for errors by external code which are not covered by the other error
	/// types; watchexec-internal errors should never use this.
	#[error("external(runtime): {0}")]
	#[diagnostic(code(watchexec::runtime::external))]
	External(#[from] Box<dyn std::error::Error + Send + Sync>),

	/// Generic I/O error, with some context.
	#[error("io({about}): {err}")]
	#[diagnostic(code(watchexec::runtime::io_error))]
	IoError {
		/// What it was about.
		about: &'static str,

		/// The I/O error which occurred.
		#[source]
		err: std::io::Error,
	},

	/// Events from the filesystem watcher event source.
	#[error("{kind:?} fs watcher error")]
	#[diagnostic(code(watchexec::runtime::fs_watcher))]
	FsWatcher {
		/// The kind of watcher that failed to instantiate.
		kind: Watcher,

		/// The underlying error.
		#[source]
		err: super::FsWatcherError,
	},

	/// Opaque internal error from a command supervisor.
	#[error("internal: command supervisor: {0}")]
	#[diagnostic(code(watchexec::runtime::internal_supervisor))]
	InternalSupervisor(String),

	/// Error received when an event cannot be sent to the event channel.
	#[error("cannot send event from {ctx}: {err}")]
	#[diagnostic(code(watchexec::runtime::event_channel_send))]
	EventChannelSend {
		/// The context in which this error happened.
		///
		/// This is not stable and its value should not be relied on except for printing the error.
		ctx: &'static str,

		/// The underlying error.
		#[source]
		err: mpsc::error::SendError<Event>,
	},

	/// Error received when an event cannot be sent to the event channel.
	#[error("cannot send event from {ctx}: {err}")]
	#[diagnostic(code(watchexec::runtime::event_channel_try_send))]
	EventChannelTrySend {
		/// The context in which this error happened.
		///
		/// This is not stable and its value should not be relied on except for printing the error.
		ctx: &'static str,

		/// The underlying error.
		#[source]
		err: mpsc::error::TrySendError<Event>,
	},

	/// Error received when a [`Handler`][crate::handler::Handler] errors.
	///
	/// The error is completely opaque, having been flattened into a string at the error point.
	#[error("handler error while {ctx}: {err}")]
	#[diagnostic(code(watchexec::runtime::handler))]
	Handler {
		/// The context in which this error happened.
		///
		/// This is not stable and its value should not be relied on except for printing the error.
		ctx: &'static str,

		/// The underlying error, as the Display representation of the original error.
		err: String,
	},

	/// Error received when a [`Handler`][crate::handler::Handler] which has been passed a lock has kept that lock open after the handler has completed.
	#[error("{0} handler returned while holding a lock alive")]
	#[diagnostic(code(watchexec::runtime::handler_lock_held))]
	HandlerLockHeld(&'static str),

	/// Error received when operating on a process.
	#[error("when operating on process: {0}")]
	#[diagnostic(code(watchexec::runtime::process))]
	Process(#[source] std::io::Error),

	/// Error received when a process did not start correctly, or finished before we could even tell.
	#[error("process was dead on arrival")]
	#[diagnostic(code(watchexec::runtime::process_doa))]
	ProcessDeadOnArrival,

	/// Error received when a [`SubSignal`] is unsupported
	///
	/// This may happen if the signal is not supported on the current platform, or if Watchexec
	/// doesn't support sending the signal.
	#[error("unsupported signal: {0:?}")]
	#[diagnostic(code(watchexec::runtime::unsupported_signal))]
	UnsupportedSignal(SubSignal),

	/// Error received when clearing the screen.
	#[error("clear screen: {0}")]
	#[diagnostic(code(watchexec::runtime::clearscreen))]
	Clearscreen(#[from] clearscreen::Error),

	/// Error received when parsing a glob (possibly from an [`IgnoreFile`]) fails.
	///
	/// [`IgnoreFile`]: crate::ignore::IgnoreFile
	#[error("cannot parse glob from ignore '{file:?}': {err}")]
	#[diagnostic(code(watchexec::runtime::ignore_glob))]
	GlobsetGlob {
		/// The path to the erroring ignore file.
		file: Option<PathBuf>,

		/// The underlying error.
		#[source]
		err: ignore::Error,
		// TODO: extract glob error into diagnostic
	},

	/// Error received when an [`IgnoreFile`] cannot be read.
	///
	/// [`IgnoreFile`]: crate::ignore::IgnoreFile
	#[error("cannot read ignore '{file}': {err}")]
	#[diagnostic(code(watchexec::runtime::ignore_file_read))]
	IgnoreFileRead {
		/// The path to the erroring ignore file.
		file: PathBuf,

		/// The underlying error.
		#[source]
		err: std::io::Error,
	},

	/// Error emitted by a [`Filterer`](crate::filter::Filterer).
	///
	/// With built-in filterers this will probably be a dynbox of
	/// [`TaggedFiltererError`](crate::error::TaggedFiltererError), but it is
	/// possible to use a custom filterer which emits a different error type.
	#[error("{kind} filterer: {err}")]
	#[diagnostic(code(watchexec::runtime::filterer))]
	Filterer {
		/// The kind of filterer that failed.
		///
		/// This should be set by the filterer itself to a short name for the filterer.
		///
		/// This is not stable and its value should not be relied on except for printing the error.
		kind: &'static str,

		/// The underlying error.
		#[source]
		err: Box<dyn std::error::Error + Send + Sync>,
	},

	/// A set of related [`RuntimeError`]s.
	#[error("related: {0:?}")]
	#[diagnostic(code(watchexec::runtime::set))]
	Set(#[related] Vec<RuntimeError>),
}
