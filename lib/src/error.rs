//! Error types for critical, runtime, and specialised errors.

use std::{collections::HashMap, path::PathBuf};

use miette::Diagnostic;
use thiserror::Error;
use tokio::{
	sync::{mpsc, watch},
	task::JoinError,
};

use crate::{
	action,
	event::Event,
	filter::tagged::{Filter, Matcher},
	fs::{self, Watcher},
};

/// Errors which are not recoverable and stop watchexec execution.
#[derive(Debug, Diagnostic, Error)]
#[non_exhaustive]
pub enum CriticalError {
	/// Pseudo-error used to signal a graceful exit.
	#[error("this should never be printed (exit)")]
	#[diagnostic(code(watchexec::runtime::exit))]
	Exit,

	/// A critical I/O error occurred.
	#[error(transparent)]
	#[diagnostic(code(watchexec::critical::io_error))]
	IoError(#[from] std::io::Error),

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

/// Errors which _may_ be recoverable, transient, or only affect a part of the operation, and should
/// be reported to the user and/or acted upon programatically, but will not outright stop watchexec.
#[derive(Debug, Diagnostic, Error)]
#[non_exhaustive]
pub enum RuntimeError {
	/// Pseudo-error used to signal a graceful exit.
	#[error("this should never be printed (exit)")]
	#[diagnostic(code(watchexec::runtime::exit))]
	Exit,

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

	/// Opaque internal error from a command supervisor.
	#[error("internal: command supervisor: {0}")]
	#[diagnostic(code(watchexec::runtime::internal_supervisor))]
	InternalSupervisor(String),

	/// Error received when an event cannot be sent to the event channel.
	#[error("cannot send event from {ctx}: {err}")]
	#[diagnostic(code(watchexec::runtime::event_channel_send))]
	EventChannelSend {
		ctx: &'static str,
		#[source]
		err: mpsc::error::SendError<Event>,
	},

	/// Error received when an event cannot be sent to the event channel.
	#[error("cannot send event from {ctx}: {err}")]
	#[diagnostic(code(watchexec::runtime::event_channel_try_send))]
	EventChannelTrySend {
		ctx: &'static str,
		#[source]
		err: mpsc::error::TrySendError<Event>,
	},

	/// Error received when a [`Handler`][crate::handler::Handler] errors.
	///
	/// The error is completely opaque, having been flattened into a string at the error point.
	#[error("handler error while {ctx}: {err}")]
	#[diagnostic(code(watchexec::runtime::handler))]
	Handler { ctx: &'static str, err: String },

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

	/// Error received when clearing the screen.
	#[error("clear screen: {0}")]
	#[diagnostic(code(watchexec::runtime::clearscreen))]
	Clearscreen(#[from] clearscreen::Error),

	/// Error received when a tagged filter cannot be parsed.
	#[error("cannot parse filter `{src}`: {err:?}")]
	#[diagnostic(code(watchexec::runtime::filter_parse))]
	FilterParse {
		src: String,
		err: nom::error::ErrorKind,
		// TODO: use miette's source snippet feature
	},

	/// Error received when a filter cannot be added or removed from a tagged filter list.
	#[error("cannot {action} filter: {err:?}")]
	#[diagnostic(code(watchexec::runtime::filter_change))]
	FilterChange {
		action: &'static str,
		#[source]
		err: watch::error::SendError<HashMap<Matcher, Vec<Filter>>>,
	},
}

/// Errors occurring from reconfigs.
#[derive(Debug, Diagnostic, Error)]
#[non_exhaustive]
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
