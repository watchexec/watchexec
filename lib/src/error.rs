//! Watchexec has two error types: for critical and for runtime errors.

use std::path::PathBuf;

use miette::Diagnostic;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::{event::Event, fs::Watcher};

/// Errors which are not recoverable and stop watchexec execution.
#[derive(Debug, Diagnostic, Error)]
#[non_exhaustive]
pub enum CriticalError {
	/// A critical I/O error occurred.
	#[error(transparent)]
	#[diagnostic(code(watchexec::critical::io_error))]
	IoError(#[from] std::io::Error),

	/// Error received when an event cannot be sent to the errors channel.
	#[error("cannot send internal runtime error: {0}")]
	#[diagnostic(code(watchexec::critical::error_channel_send))]
	ErrorChannelSend(#[from] mpsc::error::SendError<RuntimeError>),
}

/// Errors which _may_ be recoverable, transient, or only affect a part of the operation, and should
/// be reported to the user and/or acted upon programatically, but will not outright stop watchexec.
#[derive(Debug, Diagnostic, Error)]
#[non_exhaustive]
pub enum RuntimeError {
	/// Generic I/O error, with no additional context.
	#[error(transparent)]
	#[diagnostic(code(watchexec::runtime::io_error))]
	IoError(#[from] std::io::Error),

	/// Error received when creating a filesystem watcher fails.
	#[error("{kind:?} watcher failed to instantiate: {err}")]
	#[diagnostic(
		code(watchexec::runtime::fs_watcher_error),
		help("perhaps retry with the poll watcher")
	)]
	FsWatcherCreate {
		kind: Watcher,
		#[source]
		err: notify::Error,
	},

	/// Error received when reading a filesystem event fails.
	#[error("{kind:?} watcher received an event that we could not read: {err}")]
	#[diagnostic(code(watchexec::runtime::fs_watcher_event))]
	FsWatcherEvent {
		kind: Watcher,
		#[source]
		err: notify::Error,
	},

	/// Error received when adding to the pathset for the filesystem watcher fails.
	#[error("while adding {path:?} to the {kind:?} watcher: {err}")]
	#[diagnostic(code(watchexec::runtime::fs_watcher_path_add))]
	FsWatcherPathAdd {
		path: PathBuf,
		kind: Watcher,
		#[source]
		err: notify::Error,
	},

	/// Error received when removing from the pathset for the filesystem watcher fails.
	#[error("while removing {path:?} from the {kind:?} watcher: {err}")]
	#[diagnostic(code(watchexec::runtime::fs_watcher_path_remove))]
	FsWatcherPathRemove {
		path: PathBuf,
		kind: Watcher,
		#[source]
		err: notify::Error,
	},

	/// Error received when an event cannot be sent to the event channel.
	#[error("cannot send event from {ctx}: {err}")]
	#[diagnostic(code(watchexec::runtime::event_channel_send))]
	EventChannelSend {
		ctx: &'static str,
		#[source]
		err: mpsc::error::TrySendError<Event>,
	},
}
