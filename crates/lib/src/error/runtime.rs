use miette::Diagnostic;
use thiserror::Error;
use watchexec_events::{Event, Priority};
use watchexec_signals::Signal;

use crate::fs::Watcher;

/// Errors which _may_ be recoverable, transient, or only affect a part of the operation, and should
/// be reported to the user and/or acted upon programmatically, but will not outright stop watchexec.
///
/// Some errors that are classified here are spurious and may be ignored. For example,
/// "waiting on process" errors should not be printed to the user by default:
///
/// ```
/// # use tracing::error;
/// # use watchexec::{Config, ErrorHook, error::RuntimeError};
/// # let mut config = Config::default();
/// config.on_error(|err: ErrorHook| {
///     if let RuntimeError::IoError {
///         about: "waiting on process group",
///         ..
///     } = err.error
///     {
///         error!("{}", err.error);
///         return;
///     }
///
///     // ...
/// });
/// ```
///
/// On the other hand, some errors may not be fatal to this library's understanding, but will be to
/// your application. In those cases, you should "elevate" these errors, which will transform them
/// to [`CriticalError`](super::CriticalError)s:
///
/// ```
/// # use watchexec::{Config, ErrorHook, error::{RuntimeError, FsWatcherError}};
/// # let mut config = Config::default();
/// config.on_error(|err: ErrorHook| {
///     if let RuntimeError::FsWatcher {
///         err:
///             FsWatcherError::Create { .. }
///             | FsWatcherError::TooManyWatches { .. }
///             | FsWatcherError::TooManyHandles { .. },
///         ..
///     } = err.error {
///         err.elevate();
///         return;
///     }
///
///     // ...
/// });
/// ```
#[derive(Debug, Diagnostic, Error)]
#[non_exhaustive]
pub enum RuntimeError {
	/// Pseudo-error used to signal a graceful exit.
	#[error("this should never be printed (exit)")]
	Exit,

	/// For custom runtime errors.
	///
	/// This should be used for errors by external code which are not covered by the other error
	/// types; watchexec-internal errors should never use this.
	#[error("external(runtime): {0}")]
	External(#[from] Box<dyn std::error::Error + Send + Sync>),

	/// Generic I/O error, with some context.
	#[error("io({about}): {err}")]
	IoError {
		/// What it was about.
		about: &'static str,

		/// The I/O error which occurred.
		#[source]
		err: std::io::Error,
	},

	/// Events from the filesystem watcher event source.
	#[error("{kind:?} fs watcher error")]
	FsWatcher {
		/// The kind of watcher that failed to instantiate.
		kind: Watcher,

		/// The underlying error.
		#[source]
		err: super::FsWatcherError,
	},

	/// Events from the keyboard event source
	#[error("keyboard watcher error")]
	KeyboardWatcher {
		/// The underlying error.
		#[source]
		err: super::KeyboardWatcherError,
	},

	/// Opaque internal error from a command supervisor.
	#[error("internal: command supervisor: {0}")]
	InternalSupervisor(String),

	/// Error received when an event cannot be sent to the event channel.
	#[error("cannot send event from {ctx}: {err}")]
	EventChannelSend {
		/// The context in which this error happened.
		///
		/// This is not stable and its value should not be relied on except for printing the error.
		ctx: &'static str,

		/// The underlying error.
		#[source]
		err: async_priority_channel::SendError<(Event, Priority)>,
	},

	/// Error received when an event cannot be sent to the event channel.
	#[error("cannot send event from {ctx}: {err}")]
	EventChannelTrySend {
		/// The context in which this error happened.
		///
		/// This is not stable and its value should not be relied on except for printing the error.
		ctx: &'static str,

		/// The underlying error.
		#[source]
		err: async_priority_channel::TrySendError<(Event, Priority)>,
	},

	/// Error received when a [`Handler`][crate::handler::Handler] errors.
	///
	/// The error is completely opaque, having been flattened into a string at the error point.
	#[error("handler error while {ctx}: {err}")]
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
	HandlerLockHeld(&'static str),

	/// Error received when operating on a process.
	#[error("when operating on process: {0}")]
	Process(#[source] std::io::Error),

	/// Error received when a process did not start correctly, or finished before we could even tell.
	#[error("process was dead on arrival")]
	ProcessDeadOnArrival,

	/// Error received when a [`Signal`] is unsupported
	///
	/// This may happen if the signal is not supported on the current platform, or if Watchexec
	/// doesn't support sending the signal.
	#[error("unsupported signal: {0:?}")]
	UnsupportedSignal(Signal),

	/// Error received when there are no commands to run.
	///
	/// This is generally a programmer error and should be caught earlier.
	#[error("no commands to run")]
	NoCommands,

	/// Error received when trying to render a [`Command::Shell`](crate::command::Command) that has no `command`
	///
	/// This is generally a programmer error and should be caught earlier.
	#[error("empty shelled command")]
	CommandShellEmptyCommand,

	/// Error received when trying to render a [`Shell::Unix`](crate::command::Shell) with an empty shell
	///
	/// This is generally a programmer error and should be caught earlier.
	#[error("empty shell program")]
	CommandShellEmptyShell,

	/// Error received when clearing the screen.
	#[error("clear screen: {0}")]
	Clearscreen(#[from] clearscreen::Error),

	/// Error received from the [`ignore-files`](ignore_files) crate.
	#[error("ignore files: {0}")]
	IgnoreFiles(
		#[diagnostic_source]
		#[from]
		ignore_files::Error,
	),

	/// Error emitted by a [`Filterer`](crate::filter::Filterer).
	#[error("{kind} filterer: {err}")]
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
}
