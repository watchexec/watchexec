use std::{num::NonZeroU64, sync::Arc};

use async_priority_channel as priority;
use command_group::AsyncCommandGroup;
use tokio::{
	select, spawn,
	sync::{
		mpsc::{self, Sender},
		watch,
	},
};
use tracing::{debug, debug_span, error, info, trace};
use watchexec_signals::Signal;

use crate::{
	action::{PostSpawn, PreSpawn},
	command::{Command, Isolation, Program},
	error::RuntimeError,
	event::{Event, Priority, Source, Tag},
	handler::HandlerLock,
};

use super::Process;

#[derive(Clone, Copy, Debug)]
enum Intervention {
	Kill,
	Signal(Signal),
}

/// A task which supervises a sequence of processes.
///
/// This spawns processes from a vec of [`Command`]s in order and waits for each to complete while
/// handling interventions to itself: orders to terminate, or to send a signal to the current
/// process. It also immediately issues a [`Tag::ProcessCompletion`] event when the set completes.
#[derive(Debug)]
pub struct Supervisor {
	intervene: Sender<Intervention>,
	ongoing: watch::Receiver<bool>,
}

/// Defines the arguments needed to spawn a [`Supervisor`].
///
/// Used to gather all nc
pub struct Args {
	/// Error channel used to send and receive errors from the [`Supervisor`] and it's [`Command`]s.
	pub errors: Sender<RuntimeError>,
	/// Events channel used to send and receive events to and from the [`Supervisor`] and it's [`Command`]s.
	pub events: priority::Sender<Event, Priority>,
	/// The [`Command`] run by the [`Supervisor`].
	pub command: Command,
	/// The [`SupervisorId`] associated with the [`Supervisor`] that is being spawned.
	pub supervisor_id: SupervisorId,
	/// The [`Event`]s associated with the [`Supervisor`].
	pub actioned_events: Arc<[Event]>,
	/// The [`PreSpawn`] handler is executed before the [`Supervisor`] is spawned.
	pub pre_spawn_handler: HandlerLock<PreSpawn>,
	/// The [`PostSpawn`] handler is executed after the [`Supervisor`] finishes execution.
	pub post_spawn_handler: HandlerLock<PostSpawn>,
}

impl Supervisor {
	/// Spawns the command set, the supervisor task from the provided arguments and returns a new
	/// control object.
	pub fn spawn(args: Args) -> Result<Self, RuntimeError> {
		let Args {
			errors,
			events,
			mut command,
			supervisor_id,
			actioned_events,
			pre_spawn_handler,
			post_spawn_handler,
		} = args;

		let program = command
			.sequence
			.pop_front()
			.ok_or(RuntimeError::NoCommands)?;

		let (notify, waiter) = watch::channel(true);
		let (int_s, int_r) = mpsc::channel(8);

		spawn(async move {
			let span = debug_span!("supervisor");

			let mut program = program;
			let mut command = command;
			let mut int = int_r;

			loop {
				let (mut process, pid) = match span.in_scope(|| {
					spawn_process(
						program,
						supervisor_id,
						command.isolation,
						actioned_events.clone(),
						pre_spawn_handler.clone(),
						post_spawn_handler.clone(),
					)
				}) {
					Ok(pp) => pp,
					Err(err) => {
						let _enter = span.enter();
						error!(%err, "while spawning process");
						errors.send(err).await.ok();
						trace!("marking process as done");
						notify
							.send(false)
							.unwrap_or_else(|e| trace!(%e, "error sending process complete"));
						trace!("closing supervisor task early");
						return;
					}
				};

				span.in_scope(|| debug!(?process, ?pid, "spawned process"));

				loop {
					select! {
						p = process.wait() => {
							match p {
								Ok(_) => break, // deal with it below
								Err(err) => {
									let _enter = span.enter();
									error!(%err, "while waiting on process");
									errors.try_send(err).ok();
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
									let _enter = span.enter();
										error!(%err, "while killing process");
										errors.try_send(err).ok();
										trace!("continuing to watch command");
									}
								}
								#[cfg(unix)]
								Intervention::Signal(sig) => {
									let _enter = span.enter();
									if let Some(sig) = sig.to_nix() {
										if let Err(err) = process.signal(sig) {
											error!(%err, "while sending signal to process");
											errors.try_send(err).ok();
											trace!("continuing to watch command");
										}
									} else {
										let err = RuntimeError::UnsupportedSignal(sig);
										error!(%err, "while sending signal to process");
										errors.try_send(err).ok();
										trace!("continuing to watch command");
									}
								}
								#[cfg(windows)]
								Intervention::Signal(sig) => {
									let _enter = span.enter();
									// https://github.com/watchexec/watchexec/issues/219
									let err = RuntimeError::UnsupportedSignal(sig);
									error!(%err, "while sending signal to process");
									errors.try_send(err).ok();
									trace!("continuing to watch command");
								}
							}
						}
						else => break,
					}
				}

				span.in_scope(|| trace!("got out of loop, waiting once more"));
				match process.wait().await {
					Err(err) => {
						let _enter = span.enter();
						error!(%err, "while waiting on process");
						errors.try_send(err).ok();
					}
					Ok(status) => {
						let event = span.in_scope(|| {
							let event = Event {
								tags: vec![
									Tag::Source(Source::Internal),
									Tag::ProcessCompletion(status.map(Into::into)),
								],
								metadata: Default::default(),
							};

							debug!(?event, "creating synthetic process completion event");
							event
						});

						if let Err(err) = events.send(event, Priority::Low).await {
							let _enter = span.enter();
							error!(%err, "while sending process completion event");
							errors
								.try_send(RuntimeError::EventChannelSend {
									ctx: "command supervisor",
									err,
								})
								.ok();
						}
					}
				}

				// TODO: handle continue_on_error
				let _enter = span.enter();
				if let Some(prog) = command.sequence.pop_front() {
					debug!(?prog, "queuing up next program");
					program = prog;
				} else {
					debug!("no more programs to supervise");
					break;
				}
			}

			let _enter = span.enter();
			trace!("marking process as done");
			notify
				.send(false)
				.unwrap_or_else(|e| trace!(%e, "error sending process complete"));
			trace!("closing supervisor task");
		});

		Ok(Self {
			ongoing: waiter,
			intervene: int_s,
		})
	}

	/// Issues a signal to the process.
	///
	/// On Windows, this currently only supports [`Signal::ForceStop`].
	///
	/// While this is async, it returns once the signal intervention has been sent internally, not
	/// when the signal has been delivered.
	pub async fn signal(&self, signal: Signal) {
		if cfg!(windows) {
			if signal == Signal::ForceStop {
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

fn spawn_process(
	program: Program,
	supervisor_id: SupervisorId,
	isolation: Isolation,
	actioned_events: Arc<[Event]>,
	pre_spawn_handler: HandlerLock<PreSpawn>,
	post_spawn_handler: HandlerLock<PostSpawn>,
) -> Result<(Process, u32), RuntimeError> {
	debug!(?isolation, ?program, "preparing program");
	#[cfg_attr(windows, allow(unused_mut))]
	let mut spawnable = program.to_spawnable();

	// Required from Rust 1.66:
	// https://github.com/rust-lang/rust/pull/101077
	//
	// We do that before the pre-spawn so that hook can be used to set a different mask if wanted.
	#[cfg(unix)]
	{
		use nix::sys::signal::{sigprocmask, SigSet, SigmaskHow, Signal};
		unsafe {
			spawnable.pre_exec(|| {
				let mut oldset = SigSet::empty();
				let mut newset = SigSet::all();
				newset.remove(Signal::SIGHUP); // leave SIGHUP alone so nohup works
				debug!(unblocking=?newset, "resetting process sigmask");
				sigprocmask(SigmaskHow::SIG_UNBLOCK, Some(&newset), Some(&mut oldset))?;
				debug!(?oldset, "sigmask reset");
				Ok(())
			});
		}
	}

	debug!("running pre-spawn handler");
	let (payload, command) = PreSpawn::new(
		program.clone(),
		isolation,
		spawnable,
		actioned_events.clone(),
		supervisor_id,
	);
	pre_spawn_handler.call(payload);

	debug!("pre-spawn handler done, obtaining command");
	let mut spawnable = Arc::into_inner(command)
		.and_then(|mutex| mutex.into_inner().ok())
		.expect("prespawn handler lock held after prespawn handler done");

	info!(command=?spawnable, "spawning command");
	let (proc, id) = match isolation {
		Isolation::Grouped => {
			let proc = spawnable
				.group()
				.kill_on_drop(true)
				.spawn()
				.map_err(|err| RuntimeError::IoError {
					about: "spawning process group",
					err,
				})?;
			let id = proc.id().ok_or(RuntimeError::ProcessDeadOnArrival)?;
			info!(pgid=%id, "process group spawned");
			(Process::Grouped(proc), id)
		}
		Isolation::None => {
			let proc =
				spawnable
					.kill_on_drop(true)
					.spawn()
					.map_err(|err| RuntimeError::IoError {
						about: "spawning process (ungrouped)",
						err,
					})?;
			let id = proc.id().ok_or(RuntimeError::ProcessDeadOnArrival)?;
			info!(pid=%id, "process spawned");
			(Process::Ungrouped(proc), id)
		}
	};

	debug!("running post-spawn handler");
	post_spawn_handler.call(PostSpawn {
		program,
		isolation,
		events: actioned_events,
		id,
		supervisor_id,
	});
	debug!("done with post-spawn handler");

	Ok((proc, id))
}

/// Used to identify command registered with a Supervisor.
#[must_use]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct SupervisorId(NonZeroU64);

impl Default for SupervisorId {
	fn default() -> Self {
		use std::{
			collections::hash_map::RandomState,
			hash::{BuildHasher, Hasher},
		};
		// generates pseudo-random u64 using `std`'s
		// [`RandomState`](https://doc.rust-lang.org/std/collections/hash_map/struct.RandomState.html)
		let seed = RandomState::new().build_hasher().finish();

		let non_zero = seed.saturating_add(1);

		// Safety:
		// 1. The Saturating add ensures the value of `non_zero` is at least 1.
		unsafe { Self(NonZeroU64::new_unchecked(non_zero)) }
	}
}
