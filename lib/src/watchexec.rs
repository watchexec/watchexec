use std::{mem::take, sync::Arc};

use atomic_take::AtomicTake;
use futures::FutureExt;
use tokio::{
	spawn,
	sync::{mpsc, watch, Notify},
	task::{JoinError, JoinHandle},
	try_join,
};

use crate::{
	config::Config,
	error::{CriticalError, ReconfigError},
	fs, signal,
};

#[derive(Debug)]
pub struct Watchexec {
	handle: Arc<AtomicTake<JoinHandle<Result<(), CriticalError>>>>,
	start_lock: Arc<Notify>,
	fs_watch: watch::Sender<fs::WorkingData>,
}

impl Watchexec {
	/// TODO
	///
	/// Returns an [`Arc`] for convenience; use [`try_unwrap`][Arc::try_unwrap()] to get the value
	/// directly if needed.
	pub fn new(mut config: Config) -> Result<Arc<Self>, CriticalError> {
		let (fs_s, fs_r) = watch::channel(take(&mut config.fs));

		let notify = Arc::new(Notify::new());
		let start_lock = notify.clone();
		let handle = spawn(async move {
			notify.notified().await;

			let (er_s, er_r) = mpsc::channel(config.error_channel_size);
			let (ev_s, ev_r) = mpsc::channel(config.event_channel_size);

			macro_rules! subtask {
				($task:expr) => {
					spawn($task).then(|jr| async { flatten(jr) })
				};
			}

			let fs = subtask!(fs::worker(fs_r, er_s.clone(), ev_s.clone()));
			let signal = subtask!(signal::worker(er_s.clone(), ev_s.clone()));

			try_join!(fs, signal).map(drop)
		});

		Ok(Arc::new(Self {
			handle: Arc::new(AtomicTake::new(handle)),
			start_lock,
			fs_watch: fs_s,
		}))
	}

	pub fn reconfig(&self, config: Config) -> Result<(), ReconfigError> {
		self.fs_watch.send(config.fs)?;
		Ok(())
	}

	/// Start watchexec and obtain the handle to its main task.
	///
	/// This must only be called once.
	///
	/// # Panics
	/// Panics if called twice.
	pub fn main(&self) -> JoinHandle<Result<(), CriticalError>> {
		self.start_lock.notify_one();
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
