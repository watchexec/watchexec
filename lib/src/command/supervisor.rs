use std::sync::{
	atomic::{AtomicBool, Ordering},
	Arc,
};

use command_group::{AsyncCommandGroup, Signal};
use tokio::{
	process::Command,
	select, spawn,
	sync::{
		mpsc::{self, Sender},
		oneshot,
	},
	task::JoinHandle,
};
use tracing::{debug, error, trace, warn};

use crate::{
	error::RuntimeError,
	event::{Event, Particle, Source},
};

use super::Process;

#[derive(Clone, Copy, Debug)]
enum Intervention {
	Kill,
	#[cfg(unix)]
	Signal(Signal),
}

#[derive(Debug)]
pub struct Supervisor {
	id: u32,
	intervene: Sender<Intervention>,
	handle: JoinHandle<()>,

	// why this and not a watch::channel? two reasons:
	// 1. I tried the watch and ran into some race conditions???
	// 2. This way it's typed-enforced that I send only once
	waiter: Option<oneshot::Receiver<()>>,
	ongoing: Arc<AtomicBool>,
}

impl Supervisor {
	pub fn spawn(
		errors: Sender<RuntimeError>,
		events: Sender<Event>,
		command: &mut Command,
		grouped: bool,
	) -> Result<Self, RuntimeError> {
		debug!(%grouped, ?command, "spawning command");
		let (process, id) = if grouped {
			let proc = command.group_spawn()?;
			let id = proc.id().ok_or(RuntimeError::ProcessDeadOnArrival)?;
			debug!(pgid=%id, "process group spawned");
			(Process::Grouped(proc), id)
		} else {
			let proc = command.spawn()?;
			let id = proc.id().ok_or(RuntimeError::ProcessDeadOnArrival)?;
			debug!(pid=%id, "process spawned");
			(Process::Ungrouped(proc), id)
		};

		let ongoing = Arc::new(AtomicBool::new(true));
		let (notify, waiter) = oneshot::channel();
		let (int_s, int_r) = mpsc::channel(8);

		let going = ongoing.clone();
		let handle = spawn(async move {
			let mut process = process;
			let mut int = int_r;

			debug!(?process, "starting task to watch on process");

			loop {
				select! {
					p = process.wait() => {
						match p {
							Ok(_) => break, // deal with it below
							Err(err) => {
								error!(%err, "while waiting on process");
								errors.send(err).await.ok();
								trace!("marking process as done");
								going.store(false, Ordering::SeqCst);
								trace!("closing supervisor task early");
								notify.send(()).ok();
								return;
							}
						}
					},
					Some(int) = int.recv() => {
						match int {
							Intervention::Kill => {
								if let Err(err) = process.kill().await {
									error!(%err, "while killing process");
									errors.send(err).await.ok();
									trace!("continuing to watch command");
								}
							}
							#[cfg(unix)]
							Intervention::Signal(sig) => {
								if let Err(err) = process.signal(sig) {
									error!(%err, "while sending signal to process");
									errors.send(err).await.ok();
									trace!("continuing to watch command");
								}
							}
						}
					}
					else => break,
				}
			}

			trace!("got out of loop, waiting once more");
			match process.wait().await {
				Err(err) => {
					error!(%err, "while waiting on process");
					errors.send(err).await.ok();
				}
				Ok(status) => {
					let event = Event {
						particulars: vec![
							Particle::Source(Source::Internal),
							Particle::ProcessCompletion(status),
						],
						metadata: Default::default(),
					};

					debug!(?event, "creating synthetic process completion event");
					if let Err(err) = events.send(event).await {
						error!(%err, "while sending process completion event");
						errors
							.send(RuntimeError::EventChannelSend {
								ctx: "command supervisor",
								err,
							})
							.await
							.ok();
					}
				}
			}

			trace!("marking process as done");
			going.store(false, Ordering::SeqCst);
			trace!("closing supervisor task");
			notify.send(()).ok();
		});

		Ok(Self {
			id,
			waiter: Some(waiter),
			ongoing,
			intervene: int_s,
			handle, // TODO: is there anything useful to do with this? do we need to keep it?
		})
	}

	pub fn id(&self) -> u32 {
		self.id
	}

	#[cfg(unix)]
	pub async fn signal(&self, signal: Signal) {
		trace!(?signal, "sending signal intervention");
		self.intervene.send(Intervention::Signal(signal)).await.ok();
		// only errors on channel closed, and that only happens if the process is dead
	}

	pub async fn kill(&self) {
		trace!("sending kill intervention");
		self.intervene.send(Intervention::Kill).await.ok();
		// only errors on channel closed, and that only happens if the process is dead
	}

	pub fn is_running(&self) -> bool {
		let ongoing = self.ongoing.load(Ordering::SeqCst);
		trace!(?ongoing, "supervisor state");
		ongoing
	}

	pub async fn wait(&mut self) -> Result<(), RuntimeError> {
		if !self.ongoing.load(Ordering::SeqCst) {
			trace!("supervisor already completed");
			return Ok(());
		}

		if let Some(waiter) = self.waiter.take() {
			debug!("waiting on supervisor completion");
			waiter
				.await
				.map_err(|err| RuntimeError::InternalSupervisor(err.to_string()))?;
			debug!("supervisor completed");

			if !self.ongoing.swap(false, Ordering::SeqCst) {
				warn!("oneshot completed but ongoing was true, this should never happen");
			}
		} else {
			warn!("waiter is None but ongoing was true, this should never happen");
			self.ongoing.store(false, Ordering::SeqCst);
		}

		Ok(())
	}
}
