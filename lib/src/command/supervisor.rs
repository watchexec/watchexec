use command_group::{AsyncCommandGroup, Signal};
use tokio::{
	process::Command,
	select, spawn,
	sync::{
		mpsc::{self, Sender},
		watch,
	},
	task::JoinHandle,
};
use tracing::{debug, error, trace};

use crate::{
	error::RuntimeError,
	event::{Event, Particle},
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
	completion: watch::Receiver<bool>,
	intervene: Sender<Intervention>,
	handle: JoinHandle<()>,
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

		let (mark_done, completion) = watch::channel(false);
		let (int_s, int_r) = mpsc::channel(8);

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
								mark_done.send(true).ok();
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
						particulars: vec![Particle::ProcessCompletion(status)],
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
			mark_done.send(true).ok();
		});

		Ok(Self {
			id,
			completion,
			intervene: int_s,
			handle, // TODO: is there anything useful to do with this? do we need to keep it?
		})
	}

	pub fn id(&self) -> u32 {
		self.id
	}

	#[cfg(unix)]
	pub async fn signal(&self, signal: Signal) -> Result<(), RuntimeError> {
		trace!(?signal, "sending signal intervention");
		self.intervene
			.send(Intervention::Signal(signal))
			.await
			.map_err(|err| RuntimeError::InternalSupervisor(err.to_string()))
	}

	pub async fn kill(&self) -> Result<(), RuntimeError> {
		trace!("sending kill intervention");
		self.intervene
			.send(Intervention::Kill)
			.await
			.map_err(|err| RuntimeError::InternalSupervisor(err.to_string()))
	}

	pub fn is_running(&self) -> bool {
		!*self.completion.borrow()
	}

	pub async fn wait(&mut self) -> Result<(), RuntimeError> {
		debug!("waiting on supervisor completion");

		loop {
			self.completion
				.changed()
				.await
				.map_err(|err| RuntimeError::InternalSupervisor(err.to_string()))?;

			if *self.completion.borrow() {
				break;
			} else {
				debug!("got completion change event, but it wasn't done (waiting more)");
			}
		}

		debug!("supervisor completed");
		Ok(())
	}
}
