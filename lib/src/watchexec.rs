use std::{
	fmt,
	mem::{replace, take},
	sync::Arc,
};

use atomic_take::AtomicTake;
use futures::FutureExt;
use once_cell::sync::OnceCell;
use tokio::{
	spawn,
	sync::{mpsc, watch, Notify},
	task::{JoinError, JoinHandle},
	try_join,
};
use tracing::{debug, error, trace};

use crate::{
	action,
	config::{InitConfig, RuntimeConfig},
	error::{CriticalError, ReconfigError, RuntimeError},
	event::Event,
	fs,
	handler::{rte, Handler},
	signal,
};

/// The main watchexec runtime.
///
/// All this really does is tie the pieces together in one convenient interface.
///
/// It creates the correct channels, spawns every available event sources, the action worker, the
/// error hook, and provides an interface to change the runtime configuration during the runtime,
/// inject synthetic events, and wait for graceful shutdown.
pub struct Watchexec {
	handle: Arc<AtomicTake<JoinHandle<Result<(), CriticalError>>>>,
	start_lock: Arc<Notify>,

	action_watch: watch::Sender<action::WorkingData>,
	fs_watch: watch::Sender<fs::WorkingData>,

	event_input: mpsc::Sender<Event>,
}

impl fmt::Debug for Watchexec {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("Watchexec").finish_non_exhaustive()
	}
}

impl Watchexec {
	/// Instantiates a new `Watchexec` runtime from configuration.
	///
	/// Returns an [`Arc`] for convenience; use [`try_unwrap`][Arc::try_unwrap()] to get the value
	/// directly if needed.
	pub fn new(
		mut init: InitConfig,
		mut runtime: RuntimeConfig,
	) -> Result<Arc<Self>, CriticalError> {
		debug!(?init, ?runtime, pid=%std::process::id(), "initialising");

		let (ev_s, ev_r) = mpsc::channel(init.event_channel_size);
		let (ac_s, ac_r) = watch::channel(take(&mut runtime.action));
		let (fs_s, fs_r) = watch::channel(fs::WorkingData::default());

		let event_input = ev_s.clone();

		// TODO: figure out how to do this (aka start the fs work) after the main task start lock
		trace!("sending initial config to fs worker");
		fs_s.send(take(&mut runtime.fs))
			.expect("cannot send to just-created fs watch (bug)");

		trace!("creating main task");
		let notify = Arc::new(Notify::new());
		let start_lock = notify.clone();
		let handle = spawn(async move {
			trace!("waiting for start lock");
			notify.notified().await;
			debug!("starting main task");

			let (er_s, er_r) = mpsc::channel(init.error_channel_size);

			let eh = replace(&mut init.error_handler, Box::new(()) as _);

			macro_rules! subtask {
				($name:ident, $task:expr) => {{
					debug!(subtask=%stringify!($name), "spawning subtask");
					spawn($task).then(|jr| async { flatten(jr) })
				}};
			}

			let action = subtask!(
				action,
				action::worker(ac_r, er_s.clone(), ev_s.clone(), ev_r)
			);
			let fs = subtask!(fs, fs::worker(fs_r, er_s.clone(), ev_s.clone()));
			let signal = subtask!(signal, signal::source::worker(er_s.clone(), ev_s.clone()));

			let error_hook = subtask!(error_hook, error_hook(er_r, eh));

			// Use Tokio TaskSet when that lands
			try_join!(action, error_hook, fs, signal)
				.map(drop)
				.or_else(|e| {
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
		Ok(Arc::new(Self {
			handle: Arc::new(AtomicTake::new(handle)),
			start_lock,

			action_watch: ac_s,
			fs_watch: fs_s,

			event_input,
		}))
	}

	/// Applies a new [`RuntimeConfig`] to the runtime.
	pub fn reconfigure(&self, config: RuntimeConfig) -> Result<(), ReconfigError> {
		debug!(?config, "reconfiguring");
		self.action_watch.send(config.action)?;
		self.fs_watch.send(config.fs)?;
		Ok(())
	}

	/// Inputs an [`Event`] directly.
	///
	/// This can be useful for testing, for custom event sources, or for one-off action triggers
	/// (for example, on start).
	///
	/// Hint: use [`Event::default()`] to send an empty event (which won't be filtered).
	pub async fn send_event(&self, event: Event) -> Result<(), CriticalError> {
		self.event_input.send(event).await?;
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

#[inline]
fn flatten(join_res: Result<Result<(), CriticalError>, JoinError>) -> Result<(), CriticalError> {
	join_res
		.map_err(CriticalError::MainTaskJoin)
		.and_then(|x| x)
}

async fn error_hook(
	mut errors: mpsc::Receiver<RuntimeError>,
	mut handler: Box<dyn Handler<ErrorHook> + Send>,
) -> Result<(), CriticalError> {
	while let Some(err) = errors.recv().await {
		if matches!(err, RuntimeError::Exit) {
			trace!("got graceful exit request via runtime error, upgrading to crit");
			return Err(CriticalError::Exit);
		}

		error!(%err, "runtime error");

		let hook = ErrorHook::new(err);
		let crit = hook.critical.clone();
		if let Err(err) = handler.handle(hook) {
			error!(%err, "error while handling error");
			let rehook = ErrorHook::new(rte("error hook", err));
			let recrit = rehook.critical.clone();
			handler.handle(rehook).unwrap_or_else(|err| {
				error!(%err, "error while handling error of handling error");
			});
			ErrorHook::handle_crit(recrit, "error handler error handler")?;
		} else {
			ErrorHook::handle_crit(crit, "error handler")?;
		}
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

	fn handle_crit(
		crit: Arc<OnceCell<CriticalError>>,
		name: &'static str,
	) -> Result<(), CriticalError> {
		match Arc::try_unwrap(crit) {
			Err(err) => {
				error!(?err, "{} hook has an outstanding ref", name);
				Ok(())
			}
			Ok(crit) => {
				if let Some(crit) = crit.into_inner() {
					debug!(%crit, "{} output a critical error", name);
					Err(crit)
				} else {
					Ok(())
				}
			}
		}
	}

	/// Set a critical error to be emitted.
	///
	/// This takes `self` and `ErrorHook` is not `Clone`, so it's only possible to call it once.
	/// Regardless, if you _do_ manage to call it twice, it will do nothing beyond the first call.
	pub fn critical(self, critical: CriticalError) {
		self.critical.set(critical).ok();
	}
}
