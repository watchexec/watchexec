use async_priority_channel as priority;
use command_group::AsyncCommandGroup;
use tokio::{
	process::Command,
	select, spawn,
	sync::{
		mpsc::{self, Sender},
		watch,
	},
};
use tracing::{debug, error, trace};

use crate::{
	error::RuntimeError,
	event::{Event, Priority, Source, Tag},
	signal::process::SubSignal,
};

use super::Process;

#[derive(Clone, Copy, Debug)]
enum Intervention {
	Kill,
	Signal(SubSignal),
}

/// A task which supervises a process.
///
/// This spawns a process from a [`Command`] and waits for it to complete while handling
/// interventions to it: orders to terminate it, or to send a signal to it. It also immediately
/// issues a [`Tag::ProcessCompletion`] event when the process completes.
#[derive(Debug)]
pub struct Supervisor {
	id: u32,
	intervene: Sender<Intervention>,
	ongoing: watch::Receiver<bool>,
}

impl Supervisor {
	/// Spawns the command, the supervision task, and returns a new control object.
	pub fn spawn(
		errors: Sender<RuntimeError>,
		events: priority::Sender<Event, Priority>,
		command: &mut Command,
		grouped: bool,
	) -> Result<Self, RuntimeError> {
		debug!(%grouped, ?command, "spawning command");
		let (process, id) = if grouped {
			let proc = command.group_spawn().map_err(|err| RuntimeError::IoError {
				about: "spawning process group",
				err,
			})?;
			let id = proc.id().ok_or(RuntimeError::ProcessDeadOnArrival)?;
			debug!(pgid=%id, "process group spawned");
			(Process::Grouped(proc), id)
		} else {
			let proc = command.spawn().map_err(|err| RuntimeError::IoError {
				about: "spawning process (ungrouped)",
				err,
			})?;
			let id = proc.id().ok_or(RuntimeError::ProcessDeadOnArrival)?;
			debug!(pid=%id, "process spawned");
			(Process::Ungrouped(proc), id)
		};

		let (notify, waiter) = watch::channel(true);
		let (int_s, int_r) = mpsc::channel(8);

		spawn(async move {
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
								notify.send(false).unwrap_or_else(|e| trace!(%e, "error sending process complete"));
								trace!("closing supervisor task early");
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
								if let Some(sig) = sig.to_nix() {
									if let Err(err) = process.signal(sig) {
										error!(%err, "while sending signal to process");
										errors.send(err).await.ok();
										trace!("continuing to watch command");
									}
								} else {
									let err = RuntimeError::UnsupportedSignal(sig);
									error!(%err, "while sending signal to process");
									errors.send(err).await.ok();
									trace!("continuing to watch command");
								}
							}
							#[cfg(windows)]
							Intervention::Signal(sig) => {
								// https://github.com/watchexec/watchexec/issues/219
								let err = RuntimeError::UnsupportedSignal(sig);
								error!(%err, "while sending signal to process");
								errors.send(err).await.ok();
								trace!("continuing to watch command");
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
						tags: vec![
							Tag::Source(Source::Internal),
							Tag::ProcessCompletion(status.map(|s| s.into())),
						],
						metadata: Default::default(),
					};

					debug!(?event, "creating synthetic process completion event");
					if let Err(err) = events.send(event, Priority::Low).await {
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
			notify
				.send(false)
				.unwrap_or_else(|e| trace!(%e, "error sending process complete"));
			trace!("closing supervisor task");
		});

		Ok(Self {
			id,
			ongoing: waiter,
			intervene: int_s,
		})
	}

	/// Get the PID of the process or process group.
	///
	/// This always successfully returns a PID, even if the process has already exited, as the PID
	/// is held as soon as the process spawns. Take care not to use this for process manipulation
	/// once the process has exited, as the ID may have been reused already.
	pub fn id(&self) -> u32 {
		self.id
	}

	/// Issues a signal to the process.
	///
	/// On Windows, this currently only supports [`SubSignal::ForceStop`].
	///
	/// While this is async, it returns once the signal intervention has been sent internally, not
	/// when the signal has been delivered.
	pub async fn signal(&self, signal: SubSignal) {
		if cfg!(windows) {
			if let SubSignal::ForceStop = signal {
				self.intervene.send(Intervention::Kill).await.ok();
			}
		// else: https://github.com/watchexec/watchexec/issues/219
		} else {
			trace!(?signal, "sending signal intervention");
			self.intervene.send(Intervention::Signal(signal)).await.ok();
		}
		// only errors on channel closed, and that only happens if the process is dead
	}

	/// Stops the process.
	///
	/// While this is async, it returns once the signal intervention has been sent internally, not
	/// when the signal has been delivered.
	pub async fn kill(&self) {
		trace!("sending kill intervention");
		self.intervene.send(Intervention::Kill).await.ok();
		// only errors on channel closed, and that only happens if the process is dead
	}

	/// Returns true if the supervisor is still running.
	///
	/// This is almost always equivalent to whether the _process_ is still running, but may not be
	/// 100% in sync.
	pub fn is_running(&self) -> bool {
		let ongoing = *self.ongoing.borrow();
		trace!(?ongoing, "supervisor state");
		ongoing
	}

	/// Returns only when the supervisor completes.
	///
	/// This is almost always equivalent to waiting for the _process_ to complete, but may not be
	/// 100% in sync.
	pub async fn wait(&self) -> Result<(), RuntimeError> {
		if !*self.ongoing.borrow() {
			trace!("supervisor already completed (pre)");
			return Ok(());
		}

		debug!("waiting on supervisor completion");
		let mut ongoing = self.ongoing.clone();
		// never completes if ongoing is marked false in between the previous check and now!
		// TODO: select with something that sleeps a bit and rechecks the ongoing
		ongoing
			.changed()
			.await
			.map_err(|err| RuntimeError::InternalSupervisor(err.to_string()))?;
		debug!("supervisor completed");

		Ok(())
	}
}
