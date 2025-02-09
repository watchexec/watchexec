use std::{
	fmt,
	future::Future,
	sync::{Arc, OnceLock},
};

use async_priority_channel as priority;
use atomic_take::AtomicTake;
use futures::TryFutureExt;
use miette::Diagnostic;
use tokio::{
	spawn,
	sync::{mpsc, Notify},
	task::{JoinHandle, JoinSet},
};
use tracing::{debug, error, trace};
use watchexec_events::{Event, Priority};

use crate::{
	action::{self, ActionHandler},
	changeable::ChangeableFn,
	error::{CriticalError, RuntimeError},
	sources::{fs, keyboard, signal},
	Config,
};

/// The main watchexec runtime.
///
/// All this really does is tie the pieces together in one convenient interface.
///
/// It creates the correct channels, spawns every available event sources, the action worker, the
/// error hook, and provides an interface to change the runtime configuration during the runtime,
/// inject synthetic events, and wait for graceful shutdown.
pub struct Watchexec {
	/// The configuration of this Watchexec instance.
	///
	/// Configuration can be changed at any time using the provided methods on [`Config`].
	///
	/// Treat this field as readonly: replacing it with a different instance of `Config` will not do
	/// anything except potentially lose you access to the actual Watchexec config. In normal use
	/// you'll have obtained `Watchexec` behind an `Arc` so that won't be an issue.
	///
	/// # Examples
	///
	/// Change the action handler:
	///
	/// ```no_run
	/// # use watchexec::Watchexec;
	/// let wx = Watchexec::default();
	/// wx.config.on_action(|mut action| {
	///     if action.signals().next().is_some() {
	///         action.quit();
	///     }
	///
	///     action
	/// });
	/// ```
	///
	/// Set paths to be watched:
	///
	/// ```no_run
	/// # use watchexec::Watchexec;
	/// let wx = Watchexec::new(|mut action| {
	///     if action.signals().next().is_some() {
	///         action.quit();
	///     } else {
	///         for event in action.events.iter() {
	///             println!("{event:?}");
	///         }
	///     }
	///
	///     action
	/// }).unwrap();
	///
	/// wx.config.pathset(["."]);
	/// ```
	pub config: Arc<Config>,
	start_lock: Arc<Notify>,
	event_input: priority::Sender<Event, Priority>,
	handle: Arc<AtomicTake<JoinHandle<Result<(), CriticalError>>>>,
}

impl Default for Watchexec {
	/// Instantiate with default config.
	///
	/// Note that this will panic if the constructor errors.
	///
	/// Prefer calling `new()` instead.
	fn default() -> Self {
		Self::with_config(Default::default()).expect("Use Watchexec::new() to avoid this panic")
	}
}

impl fmt::Debug for Watchexec {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("Watchexec").finish_non_exhaustive()
	}
}

impl Watchexec {
	/// Instantiates a new `Watchexec` runtime given an initial action handler.
	///
	/// Returns an [`Arc`] for convenience; use [`try_unwrap`][Arc::try_unwrap()] to get the value
	/// directly if needed, or use `new_with_config`.
	///
	/// Look at the [`Config`] documentation for more on the required action handler.
	/// Watchexec will subscribe to most signals sent to the process it runs in and send them, as
	/// [`Event`]s, to the action handler. At minimum, you should check for interrupt/ctrl-c events
	/// and call `action.quit()` in your handler, otherwise hitting ctrl-c will do nothing.
	pub fn new(
		action_handler: impl (Fn(ActionHandler) -> ActionHandler) + Send + Sync + 'static,
	) -> Result<Arc<Self>, CriticalError> {
		let config = Config::default();
		config.on_action(action_handler);
		Self::with_config(config).map(Arc::new)
	}

	/// Instantiates a new `Watchexec` runtime given an initial async action handler.
	///
	/// This is the same as [`new`](fn@Self::new) except the action handler is async.
	pub fn new_async(
		action_handler: impl (Fn(ActionHandler) -> Box<dyn Future<Output = ActionHandler> + Send + Sync>)
			+ Send
			+ Sync
			+ 'static,
	) -> Result<Arc<Self>, CriticalError> {
		let config = Config::default();
		config.on_action_async(action_handler);
		Self::with_config(config).map(Arc::new)
	}

	/// Instantiates a new `Watchexec` runtime with a config.
	///
	/// This is generally not needed: the config can be changed after instantiation (before and
	/// after _starting_ Watchexec with `main()`). The only time this should be used is to set the
	/// "unchangeable" configuration items for internal details like buffer sizes for queues, or to
	/// obtain Self unwrapped by an Arc like `new()` does.
	pub fn with_config(config: Config) -> Result<Self, CriticalError> {
		debug!(?config, pid=%std::process::id(), version=%env!("CARGO_PKG_VERSION"), "initialising");
		let config = Arc::new(config);
		let outer_config = config.clone();

		let notify = Arc::new(Notify::new());
		let start_lock = notify.clone();

		let (ev_s, ev_r) =
			priority::bounded(config.event_channel_size.try_into().unwrap_or(u64::MAX));
		let event_input = ev_s.clone();

		trace!("creating main task");
		let handle = spawn(async move {
			trace!("waiting for start lock");
			notify.notified().await;
			debug!("starting main task");

			let (er_s, er_r) = mpsc::channel(config.error_channel_size);

			let mut tasks = JoinSet::new();

			tasks.spawn(action::worker(config.clone(), er_s.clone(), ev_r).map_ok(|()| "action"));
			tasks.spawn(fs::worker(config.clone(), er_s.clone(), ev_s.clone()).map_ok(|()| "fs"));
			tasks.spawn(
				signal::worker(config.clone(), er_s.clone(), ev_s.clone()).map_ok(|()| "signal"),
			);
			tasks.spawn(
				keyboard::worker(config.clone(), er_s.clone(), ev_s.clone())
					.map_ok(|()| "keyboard"),
			);
			tasks.spawn(error_hook(er_r, config.error_handler.clone()).map_ok(|()| "error"));

			while let Some(Ok(res)) = tasks.join_next().await {
				match res {
					Ok("action") => {
						debug!("action worker exited, ending watchexec");
						break;
					}
					Ok(task) => {
						debug!(task, "worker exited");
					}
					Err(CriticalError::Exit) => {
						trace!("got graceful exit request via critical error, erasing the error");
						// Close event channel to signal worker task to stop
						ev_s.close();
					}
					Err(e) => {
						return Err(e);
					}
				}
			}

			debug!("main task graceful exit");
			tasks.shutdown().await;
			Ok(())
		});

		trace!("done with setup");
		Ok(Self {
			config: outer_config,
			start_lock,
			event_input,
			handle: Arc::new(AtomicTake::new(handle)),
		})
	}

	/// Inputs an [`Event`] directly.
	///
	/// This can be useful for testing, for custom event sources, or for one-off action triggers
	/// (for example, on start).
	///
	/// Hint: use [`Event::default()`] to send an empty event (which won't be filtered).
	pub async fn send_event(&self, event: Event, priority: Priority) -> Result<(), CriticalError> {
		self.event_input.send(event, priority).await?;
		Ok(())
	}

	/// Start watchexec and obtain the handle to its main task.
	///
	/// This must only be called once.
	///
	/// # Panics
	/// Panics if called twice.
	pub fn main(&self) -> JoinHandle<Result<(), CriticalError>> {
		trace!("notifying start lock");
		self.start_lock.notify_one();

		debug!("handing over main task handle");
		self.handle
			.take()
			.expect("Watchexec::main was called twice")
	}
}

async fn error_hook(
	mut errors: mpsc::Receiver<RuntimeError>,
	handler: ChangeableFn<ErrorHook, ()>,
) -> Result<(), CriticalError> {
	while let Some(err) = errors.recv().await {
		if matches!(err, RuntimeError::Exit) {
			trace!("got graceful exit request via runtime error, upgrading to crit");
			return Err(CriticalError::Exit);
		}

		error!(%err, "runtime error");
		let payload = ErrorHook::new(err);
		let crit = payload.critical.clone();
		handler.call(payload);
		ErrorHook::handle_crit(crit)?;
	}

	Ok(())
}

/// The environment given to the error handler.
///
/// This deliberately does not implement Clone to make it hard to move it out of the handler, which
/// you should not do.
///
/// The [`ErrorHook::critical()`] method should be used to send a [`CriticalError`], which will
/// terminate watchexec. This is useful to e.g. upgrade certain errors to be fatal.
///
/// Note that returning errors from the error handler does not result in critical errors.
#[derive(Debug)]
pub struct ErrorHook {
	/// The runtime error for which this handler was called.
	pub error: RuntimeError,
	critical: Arc<OnceLock<CriticalError>>,
}

impl ErrorHook {
	fn new(error: RuntimeError) -> Self {
		Self {
			error,
			critical: Default::default(),
		}
	}

	fn handle_crit(crit: Arc<OnceLock<CriticalError>>) -> Result<(), CriticalError> {
		match Arc::try_unwrap(crit) {
			Err(err) => {
				error!(?err, "error handler hook has an outstanding ref");
				Ok(())
			}
			Ok(crit) => crit.into_inner().map_or_else(
				|| Ok(()),
				|crit| {
					debug!(%crit, "error handler output a critical error");
					Err(crit)
				},
			),
		}
	}

	/// Set a critical error to be emitted.
	///
	/// This takes `self` and `ErrorHook` is not `Clone`, so it's only possible to call it once.
	/// Regardless, if you _do_ manage to call it twice, it will do nothing beyond the first call.
	pub fn critical(self, critical: CriticalError) {
		self.critical.set(critical).ok();
	}

	/// Elevate the current runtime error to critical.
	///
	/// This is a shorthand method for `ErrorHook::critical(CriticalError::Elevated(error))`.
	pub fn elevate(self) {
		let Self { error, critical } = self;
		critical
			.set(CriticalError::Elevated {
				help: error.help().map(|h| h.to_string()),
				err: error,
			})
			.ok();
	}
}
