use std::{
	fmt,
	future::Future,
	ops::{Deref, DerefMut},
	pin::Pin,
	sync::Arc,
	task::{Context, Poll},
};

use async_priority_channel as priority;
use atomic_take::AtomicTake;
use miette::Diagnostic;
use once_cell::sync::OnceCell;
use tokio::{
	spawn,
	sync::{mpsc, Notify},
	task::JoinHandle,
	try_join,
};
use tracing::{debug, error, trace};
use watchexec_events::{Event, Priority};

use crate::{
	action::{self, Action},
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
	/// # use watchexec::{action::Action, Watchexec};
	/// let wx = Watchexec::default();
	/// wx.config.on_action(|action: Action| {
	///     if action.signals().next().is_some() {
	///         action.quit();
	///     }
	/// });
	/// ```
	///
	/// Set paths to be watched:
	///
	/// ```no_run
	/// # use watchexec::{action::Action, Watchexec};
	/// let wx = Watchexec::new(|action: Action| {
	///     if action.signals().next().is_some() {
	///         action.quit();
	///         return;
	///     }
	///
	///     for event in action.events.iter() {
	///         println!("{event:?}");
	///     }
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
		action_handler: impl (Fn(Action) -> Action) + Send + Sync + 'static,
	) -> Result<Arc<Self>, CriticalError> {
		let config = Config::default();
		config.on_action(action_handler);
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

		let (ev_s, ev_r) = priority::bounded(config.event_channel_size);
		let event_input = ev_s.clone();

		trace!("creating main task");
		let handle = spawn(async move {
			trace!("waiting for start lock");
			notify.notified().await;
			debug!("starting main task");

			let (er_s, er_r) = mpsc::channel(config.error_channel_size);

			let action =
				SubTask::spawn("action", action::worker(config.clone(), er_s.clone(), ev_r));
			let fs = SubTask::spawn("fs", fs::worker(config.clone(), er_s.clone(), ev_s.clone()));
			let signal = SubTask::spawn(
				"signal",
				signal::worker(config.clone(), er_s.clone(), ev_s.clone()),
			);
			let keyboard = SubTask::spawn(
				"keyboard",
				keyboard::worker(config.clone(), er_s.clone(), ev_s.clone()),
			);

			let error_hook =
				SubTask::spawn("error_hook", error_hook(er_r, config.error_handler.clone()));

			// Use Tokio TaskSet when that lands
			try_join!(action, error_hook, fs, signal, keyboard)
				.map(drop)
				.or_else(|e| {
					// Close event channel to signal worker task to stop
					ev_s.close();

					if matches!(e, CriticalError::Exit) {
						trace!("got graceful exit request via critical error, erasing the error");
						Ok(())
					} else {
						Err(e)
					}
				})
				.map(|_| {
					debug!("main task graceful exit");
				})
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
	critical: Arc<OnceCell<CriticalError>>,
}

impl ErrorHook {
	fn new(error: RuntimeError) -> Self {
		Self {
			error,
			critical: Default::default(),
		}
	}

	fn handle_crit(crit: Arc<OnceCell<CriticalError>>) -> Result<(), CriticalError> {
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

#[derive(Debug)]
struct SubTask {
	name: &'static str,
	handle: JoinHandle<Result<(), CriticalError>>,
}

impl SubTask {
	pub fn spawn(
		name: &'static str,
		task: impl Future<Output = Result<(), CriticalError>> + Send + 'static,
	) -> Self {
		debug!(subtask=%name, "spawning subtask");
		Self {
			name,
			handle: spawn(task),
		}
	}
}

impl Drop for SubTask {
	fn drop(&mut self) {
		debug!(subtask=%self.name, "aborting subtask");
		self.handle.abort();
	}
}

impl Deref for SubTask {
	type Target = JoinHandle<Result<(), CriticalError>>;

	fn deref(&self) -> &Self::Target {
		&self.handle
	}
}

impl DerefMut for SubTask {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.handle
	}
}

impl Future for SubTask {
	type Output = Result<(), CriticalError>;

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		let subtask = self.name;
		match Pin::new(&mut Pin::into_inner(self).handle).poll(cx) {
			Poll::Pending => Poll::Pending,
			Poll::Ready(join_res) => {
				debug!(%subtask, "finishing subtask");
				Poll::Ready(
					join_res
						.map_err(CriticalError::MainTaskJoin)
						.and_then(|x| x),
				)
			}
		}
	}
}
