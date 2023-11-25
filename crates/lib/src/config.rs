//! Configuration and builders for [`crate::Watchexec`].

use std::{path::Path, pin::pin, sync::Arc, time::Duration};

use tokio::sync::Notify;
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
	/// This is set by the change methods whenever they're called, and notifies Watchexec that it
	/// should read the configuration again.
	pub(crate) change_signal: Arc<Notify>,

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
	pub pathset: Changeable<Vec<WatchedPath>>,

	/// The kind of filesystem watcher to be used.
	pub file_watcher: Changeable<Watcher>,

	/// Watch stdin and emit events when input comes in over the keyboard.
	///
	/// If this is true, the keyboard event source is started and configured to report when input
	/// is received on stdin. If it becomes false, the keyboard event source is shut down and stdin
	/// may flow to commands again.
	///
	/// Currently only EOF is watched for and emitted.
	pub keyboard_events: Changeable<bool>,

	/// How long to wait for events to build up before executing an action.
	///
	/// This is sometimes called "debouncing." We debounce on the trailing edge: an action is
	/// triggered only after that amount of time has passed since the first event in the cycle. The
	/// action is called with all the collected events in the cycle.
	///
	/// Default is 50ms.
	pub throttle: Changeable<Duration>,

	/// The filterer implementation to use when filtering events.
	///
	/// The default is a no-op, which will always pass every event.
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
}

impl Default for Config {
	fn default() -> Self {
		Self {
			change_signal: Default::default(),
			action_handler: ChangeableFn::new(ActionReturn::Sync),
			error_handler: Default::default(),
			pathset: Default::default(),
			file_watcher: Default::default(),
			keyboard_events: Default::default(),
			throttle: Changeable::new(Duration::from_millis(50)),
			filterer: Default::default(),
			error_channel_size: 64,
			event_channel_size: 4096,
		}
	}
}

impl Config {
	/// Signal that the configuration has changed.
	///
	/// This is called automatically by all other methods here, so most of the time calling this
	/// isn't needed, but it can be useful for some advanced uses.
	pub fn signal_change(&self) -> &Self {
		self.change_signal.notify_waiters();
		self
	}

	/// Watch the config for a change, but run once first.
	///
	/// This returns a Stream where the first value is available immediately, and then every
	/// subsequent one is from a change signal for this Config.
	#[must_use]
	pub(crate) fn watch(&self) -> ConfigWatched {
		ConfigWatched::new(self.change_signal.clone())
	}

	/// Set the pathset to be watched.
	pub fn pathset<I, P>(&self, pathset: I) -> &Self
	where
		I: IntoIterator<Item = P>,
		P: AsRef<Path>,
	{
		let pathset = pathset.into_iter().map(|p| p.as_ref().into()).collect();
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
	pub fn filterer(&self, filterer: impl Filterer + Send + Sync + 'static) -> &Self {
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
}

#[derive(Debug)]
pub(crate) struct ConfigWatched {
	first_run: bool,
	notify: Arc<Notify>,
}

impl ConfigWatched {
	fn new(notify: Arc<Notify>) -> Self {
		let notified = notify.notified();
		pin!(notified).as_mut().enable();

		Self {
			first_run: true,
			notify,
		}
	}

	pub async fn next(&mut self) {
		let notified = self.notify.notified();
		let mut notified = pin!(notified);
		notified.as_mut().enable();

		if self.first_run {
			trace!("ConfigWatched: first run");
			self.first_run = false;
		} else {
			trace!(?notified, "ConfigWatched: waiting for change");
			// there's a bit of a gotcha where any config changes made after a Notified resolves
			// but before a new one is issued will not be caught. not sure how to fix that yet.
			notified.await;
		}
	}
}
