//! Configuration and builders for [`crate::Watchexec`].

use std::{future::Future, time::Duration};

use tokio::sync::watch;
use tracing::{debug, trace};

use crate::{
	action::{ActionHandler, ActionReturn},
	changeable::{Changeable, ChangeableFn},
	filter::{ChangeableFilterer, Filterer},
	sources::fs::{WatchedPath, Watcher},
	ErrorHook,
};

/// Configuration for [`Watchexec`][crate::Watchexec].
///
/// Almost every field is a [`Changeable`], such that its value can be changed from a `&self`.
///
/// Fields are public for advanced use, but in most cases changes should be made through the
/// methods provided: not only are they more convenient, each calls `debug!` on the new value,
/// providing a quick insight into what your application sets.
///
/// The methods also set the "change signal" of the Config: this notifies some parts of Watchexec
/// they should re-read the config. If you modify values via the fields directly, you should call
/// `signal_change()` yourself. Note that this doesn't mean that changing values _without_ calling
/// this will prevent Watchexec changing until it's called: most parts of Watchexec take a
/// "just-in-time" approach and read a config item immediately before it's needed, every time it's
/// needed, and thus don't need to listen for the change signal.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Config {
	/// This monotonic revision is incremented by the change methods whenever they're called, and
	/// notifies Watchexec that it should read the configuration again.
	pub(crate) change_signal: watch::Sender<u64>,

	/// The main handler to define: what to do when an action is triggered.
	///
	/// This handler is called with the [`Action`] environment, look at its doc for more detail.
	///
	/// If this handler is not provided, or does nothing, Watchexec in turn will do nothing, not
	/// even quit. Hence, you really need to provide a handler. This is enforced when using
	/// [`Watchexec::new()`], but not when using [`Watchexec::default()`].
	///
	/// It is possible to change the handler or any other configuration inside the previous handler.
	/// This and other handlers are fetched "just in time" when needed, so changes to handlers can
	/// appear instant, or may lag a little depending on lock contention, but a handler being called
	/// does not hold its lock. A handler changing while it's being called doesn't affect the run of
	/// a previous version of the handler: it will neither be stopped nor retried with the new code.
	///
	/// It is important for this handler to return quickly: avoid performing blocking work in it.
	/// This is true for all handlers, but especially for this one, as it will block the event loop
	/// and you'll find that the internal event queues quickly fill up and it all grinds to a halt.
	/// Spawn threads or tasks, or use channels or other async primitives to communicate with your
	/// expensive code.
	pub action_handler: ChangeableFn<ActionHandler, ActionReturn>,

	/// Runtime error handler.
	///
	/// This is run on every runtime error that occurs within Watchexec. The default handler
	/// is a no-op.
	///
	/// # Examples
	///
	/// Set the error handler:
	///
	/// ```
	/// # use watchexec::{config::Config, ErrorHook};
	/// let mut config = Config::default();
	/// config.on_error(|err: ErrorHook| {
	///     tracing::error!("{}", err.error);
	/// });
	/// ```
	///
	/// Output a critical error (which will terminate Watchexec):
	///
	/// ```
	/// # use watchexec::{config::Config, ErrorHook, error::{CriticalError, RuntimeError}};
	/// let mut config = Config::default();
	/// config.on_error(|err: ErrorHook| {
	///     tracing::error!("{}", err.error);
	///
	///     if matches!(err.error, RuntimeError::FsWatcher { .. }) {
	///         err.critical(CriticalError::External("fs watcher failed".into()));
	///     }
	/// });
	/// ```
	///
	/// Elevate a runtime error to critical (will preserve the error information):
	///
	/// ```
	/// # use watchexec::{config::Config, ErrorHook, error::RuntimeError};
	/// let mut config = Config::default();
	/// config.on_error(|err: ErrorHook| {
	///     tracing::error!("{}", err.error);
	///
	///     if matches!(err.error, RuntimeError::FsWatcher { .. }) {
	///            err.elevate();
	///     }
	/// });
	/// ```
	///
	/// It is important for this to return quickly: avoid performing blocking work. Locking and
	/// writing to stdio is fine, but waiting on the network is a bad idea. Of course, an
	/// asynchronous log writer or separate UI thread is always a better idea than `println!` if
	/// have that ability.
	pub error_handler: ChangeableFn<ErrorHook, ()>,

	/// The set of filesystem paths to be watched.
	///
	/// If this is non-empty, the filesystem event source is started and configured to provide
	/// events for these paths. If it becomes empty, the filesystem event source is shut down.
	///
	/// Watched paths are themselves never filtered.
	pub pathset: Changeable<Vec<WatchedPath>>,

	/// The kind of filesystem watcher to be used.
	pub file_watcher: Changeable<Watcher>,

	/// Whether to follow directory symlinks when watching paths.
	///
	/// When enabled, directory symlink targets are included in recursive watches where supported.
	/// Native macOS filesystem watching does not follow directory symlinks outside the watched
	/// hierarchy.
	pub follow_symlinks: Changeable<bool>,

	/// Listen for Unix job-control signals (`SIGTSTP` and `SIGCONT`).
	///
	/// This is disabled by default because installing a `SIGTSTP` listener suppresses the operating
	/// system's default suspend behaviour. Applications which enable this must suspend themselves
	/// after handling the emitted [`Signal::TerminalSuspend`](watchexec_signals::Signal) event.
	///
	/// This has no effect on non-Unix platforms. It is unchangeable at runtime and must be set
	/// before Watchexec instantiation because Unix signal dispositions cannot be restored after a
	/// Tokio signal listener is installed.
	pub signal_job_control: bool,

	/// Watch stdin and emit events when input comes in over the keyboard.
	///
	/// If this is true, the keyboard event source is started and stdin is switched to raw mode
	/// (disabling line buffering). Individual key events are emitted, as well as EOF. If it
	/// becomes false, the keyboard event source is shut down, cooked mode is restored, and stdin
	/// may flow to commands again.
	///
	/// This requires a TTY and is opt-in.
	pub keyboard_events: Changeable<bool>,

	/// How long to wait for events to build up before executing an action.
	///
	/// This is sometimes called "debouncing." We debounce on the trailing edge: an action is
	/// triggered only after that amount of time has passed since the first event in the cycle. The
	/// action is called with all the collected events in the cycle.
	///
	/// Default is 50ms.
	pub throttle: Changeable<Duration>,

	/// The filterer implementation used for event and source-directory filtering.
	///
	/// The default is a no-op, which passes every event and directory.
	pub filterer: ChangeableFilterer,

	/// The buffer size of the channel which carries runtime errors.
	///
	/// The default (64) is usually fine. If you expect a much larger throughput of runtime errors,
	/// or if your `error_handler` is slow, adjusting this value may help.
	///
	/// This is unchangeable at runtime and must be set before Watchexec instantiation.
	pub error_channel_size: usize,

	/// The buffer size of the channel which carries events.
	///
	/// The default (4096) is usually fine. If you expect a much larger throughput of events,
	/// adjusting this value may help.
	///
	/// This is unchangeable at runtime and must be set before Watchexec instantiation.
	pub event_channel_size: usize,

	/// Signalled by the filesystem worker after it settles reconciliation for an observed config
	/// revision. Subscribe via [`Config::fs_ready()`] before changing filesystem configuration to
	/// avoid missing or misattributing the notification.
	pub(crate) fs_ready: watch::Sender<()>,
}

impl Default for Config {
	fn default() -> Self {
		Self {
			change_signal: watch::channel(0).0,
			action_handler: ChangeableFn::new(ActionReturn::Sync),
			error_handler: Default::default(),
			pathset: Default::default(),
			file_watcher: Default::default(),
			follow_symlinks: Changeable::new(true),
			signal_job_control: false,
			keyboard_events: Default::default(),
			throttle: Changeable::new(Duration::from_millis(50)),
			filterer: Default::default(),
			error_channel_size: 64,
			event_channel_size: 4096,
			fs_ready: watch::channel(()).0,
		}
	}
}

impl Config {
	/// Signal that the configuration has changed.
	///
	/// This is called automatically by all other methods here, so most of the time calling this
	/// isn't needed, but it can be useful for some advanced uses.
	#[allow(
		clippy::must_use_candidate,
		reason = "this return can explicitly be ignored"
	)]
	pub fn signal_change(&self) -> &Self {
		self.change_signal.send_modify(|revision| {
			*revision = revision
				.checked_add(1)
				.expect("configuration revision overflow");
		});
		self
	}

	/// Watch the config for a change, but run once first.
	///
	/// This returns a Stream where the first value is available immediately, and then every
	/// subsequent one is from a change signal for this Config.
	#[must_use]
	pub(crate) fn watch(&self) -> ConfigWatched {
		ConfigWatched::new(self.change_signal.subscribe())
	}

	/// Subscribe to filesystem worker readiness notifications.
	///
	/// The receiver is notified after the filesystem worker finishes applying the latest observed
	/// configuration.
	///
	/// Path-specific failures are reported through the error handler and do not prevent readiness.
	///
	/// Notifications carry no configuration revision and may be coalesced. To wait for a change,
	/// subscribe before making it, then call `.changed().await`.
	#[must_use]
	pub fn fs_ready(&self) -> watch::Receiver<()> {
		self.fs_ready.subscribe()
	}

	/// Set the pathset to be watched.
	pub fn pathset<I, P>(&self, pathset: I) -> &Self
	where
		I: IntoIterator<Item = P>,
		P: Into<WatchedPath>,
	{
		let pathset = pathset.into_iter().map(std::convert::Into::into).collect();
		debug!(?pathset, "Config: pathset");
		self.pathset.replace(pathset);
		self.signal_change()
	}

	/// Set the file watcher type to use.
	pub fn file_watcher(&self, watcher: Watcher) -> &Self {
		debug!(?watcher, "Config: file watcher");
		self.file_watcher.replace(watcher);
		self.signal_change()
	}

	/// Set whether symlinks are followed when watching paths.
	pub fn follow_symlinks(&self, follow: bool) -> &Self {
		debug!(?follow, "Config: follow symlinks");
		self.follow_symlinks.replace(follow);
		self.signal_change()
	}

	/// Enable keyboard/stdin event source.
	pub fn keyboard_events(&self, enable: bool) -> &Self {
		debug!(?enable, "Config: keyboard");
		self.keyboard_events.replace(enable);
		self.signal_change()
	}

	/// Set the throttle.
	pub fn throttle(&self, throttle: impl Into<Duration>) -> &Self {
		let throttle = throttle.into();
		debug!(?throttle, "Config: throttle");
		self.throttle.replace(throttle);
		self.signal_change()
	}

	/// Set the filterer implementation to use.
	pub fn filterer(&self, filterer: impl Filterer + 'static) -> &Self {
		debug!(?filterer, "Config: filterer");
		self.filterer.replace(filterer);
		self.signal_change()
	}

	/// Set the runtime error handler.
	pub fn on_error(&self, handler: impl Fn(ErrorHook) + Send + Sync + 'static) -> &Self {
		debug!("Config: on_error");
		self.error_handler.replace(handler);
		self.signal_change()
	}

	/// Set the action handler.
	pub fn on_action(
		&self,
		handler: impl (Fn(ActionHandler) -> ActionHandler) + Send + Sync + 'static,
	) -> &Self {
		debug!("Config: on_action");
		self.action_handler
			.replace(move |action| ActionReturn::Sync(handler(action)));
		self.signal_change()
	}

	/// Set the action handler to a future-returning closure.
	pub fn on_action_async(
		&self,
		handler: impl (Fn(ActionHandler) -> Box<dyn Future<Output = ActionHandler> + Send + Sync>)
			+ Send
			+ Sync
			+ 'static,
	) -> &Self {
		debug!("Config: on_action_async");
		self.action_handler
			.replace(move |action| ActionReturn::Async(handler(action)));
		self.signal_change()
	}
}

#[derive(Debug)]
pub(crate) struct ConfigWatched {
	first_run: bool,
	revision: watch::Receiver<u64>,
}

impl ConfigWatched {
	const fn new(revision: watch::Receiver<u64>) -> Self {
		Self {
			first_run: true,
			revision,
		}
	}

	pub async fn next(&mut self) -> u64 {
		if self.first_run {
			let revision = *self.revision.borrow_and_update();
			trace!(revision, "ConfigWatched: first run");
			self.first_run = false;
			revision
		} else {
			trace!("ConfigWatched: waiting for change");
			self.revision
				.changed()
				.await
				.expect("configuration change sender dropped");
			let revision = *self.revision.borrow_and_update();
			trace!(revision, "ConfigWatched: changed");
			revision
		}
	}

	/// Whether a revision is already waiting to be observed.
	///
	/// Filesystem recursion uses this before every bounded state-machine step so
	/// an obsolete reconciliation cannot advance into its destructive sweep.
	pub fn pending(&self) -> bool {
		self.first_run
			|| self
				.revision
				.has_changed()
				.expect("configuration change sender dropped")
	}
}

#[cfg(test)]
mod tests {
	use futures::FutureExt as _;

	use super::Config;

	#[test]
	fn config_watch_first_run_is_immediate() {
		let config = Config::default();
		let mut watched = config.watch();

		assert_eq!(watched.next().now_or_never(), Some(0));
	}

	#[test]
	fn config_watch_waits_after_first_run() {
		let config = Config::default();
		let mut watched = config.watch();

		assert!(watched.next().now_or_never().is_some());
		assert!(watched.next().now_or_never().is_none());
	}

	#[test]
	fn config_watch_observes_change_between_calls() {
		let config = Config::default();
		let mut watched = config.watch();

		assert_eq!(watched.next().now_or_never(), Some(0));
		config.signal_change();
		assert_eq!(watched.next().now_or_never(), Some(1));
	}
}
