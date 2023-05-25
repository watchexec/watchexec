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
use tracing::{debug, debug_span, error, info, trace, Span};
use watchexec_signals::Signal;

use crate::{
	action::{PostSpawn, PreSpawn},
	command::Command,
	error::RuntimeError,
	event::{Event, Priority, Source, Tag},
	handler::{rte, HandlerLock},
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

pub struct Args {
	errors: Sender<RuntimeError>,
	events: priority::Sender<Event, Priority>,
	commands: Vec<Command>,
	supervisor_id: SupervisorId,
	grouped: bool,
	actioned_events: Arc<[Event]>,
	pre_spawn_handler: HandlerLock<PreSpawn>,
	post_spawn_handler: HandlerLock<PostSpawn>,
}

pub struct RequiredArgs {
	pub errors: Sender<RuntimeError>,
	pub events: priority::Sender<Event, Priority>,
	pub commands: Vec<Command>,
	pub supervisor_id: SupervisorId,
	pub grouped: bool,
	pub actioned_events: Arc<[Event]>,
	pub pre_spawn_handler: HandlerLock<PreSpawn>,
	pub post_spawn_handler: HandlerLock<PostSpawn>,
}

impl From<RequiredArgs> for Args {
	fn from(value: RequiredArgs) -> Self {
		let RequiredArgs {
			errors,
			events,
			commands,
			supervisor_id,
			grouped,
			actioned_events,
			pre_spawn_handler,
			post_spawn_handler,
		} = value;

		Self {
			errors,
			events,
			commands,
			supervisor_id,
			grouped,
			actioned_events,
			pre_spawn_handler,
			post_spawn_handler,
		}
	}
}

impl Supervisor {
	/// Spawns the command set, the supervisor task from the provided arguments and returns a new
	/// control object.
	pub fn new(args: impl Into<Args>) -> Result<Supervisor, RuntimeError> {
		let Args {
			errors,
			events,
			mut commands,
			supervisor_id,
			grouped,
			actioned_events,
			pre_spawn_handler,
			post_spawn_handler,
		} = args.into();

		// get commands in reverse order so pop() returns the next to run
		commands.reverse();
		let next = commands.pop().ok_or(RuntimeError::NoCommands)?;

		let (notify, waiter) = watch::channel(true);
		let (int_s, int_r) = mpsc::channel(8);

		spawn(async move {
			let span = debug_span!("supervisor");

			let mut next = next;
			let mut commands = commands;
			let mut int = int_r;

			loop {
				let (mut process, pid) = match spawn_process(
					span.clone(),
					next,
					supervisor_id,
					grouped,
					actioned_events.clone(),
					pre_spawn_handler.clone(),
					post_spawn_handler.clone(),
				)
				.await
				{
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

				let _enter = span.enter();
				if let Some(cmd) = commands.pop() {
					debug!(?cmd, "queuing up next command");
					next = cmd;
				} else {
					debug!("no more commands to supervise");
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

	/// Spawns the command set, the supervision task with a random [`SupervisorId`] and returns a new control object.
	pub fn spawn(
		errors: Sender<RuntimeError>,
		events: priority::Sender<Event, Priority>,
		commands: Vec<Command>,
		grouped: bool,
		actioned_events: Arc<[Event]>,
		pre_spawn_handler: HandlerLock<PreSpawn>,
		post_spawn_handler: HandlerLock<PostSpawn>,
	) -> Result<Self, RuntimeError> {
		Self::new(RequiredArgs {
			errors,
			events,
			commands,
			supervisor_id: SupervisorId::default(),
			grouped,
			actioned_events,
			pre_spawn_handler,
			post_spawn_handler,
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

async fn spawn_process(
	span: Span,
	command: Command,
	supervisor_id: SupervisorId,
	grouped: bool,
	actioned_events: Arc<[Event]>,
	pre_spawn_handler: HandlerLock<PreSpawn>,
	post_spawn_handler: HandlerLock<PostSpawn>,
) -> Result<(Process, u32), RuntimeError> {
	let (pre_spawn, spawnable) = span.in_scope::<_, Result<_, RuntimeError>>(|| {
		debug!(%grouped, ?command, "preparing command");
		#[cfg_attr(windows, allow(unused_mut))]
		let mut spawnable = command.to_spawnable()?;

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
		Ok(PreSpawn::new(
			command.clone(),
			spawnable,
			actioned_events.clone(),
			supervisor_id,
		))
	})?;

	pre_spawn_handler
		.call(pre_spawn)
		.await
		.map_err(|e| rte("action pre-spawn", e.as_ref()))?;

	let (proc, id, post_spawn) = span.in_scope::<_, Result<_, RuntimeError>>(|| {
		let mut spawnable = Arc::try_unwrap(spawnable)
			.map_err(|_| RuntimeError::HandlerLockHeld("pre-spawn"))?
			.into_inner();

		info!(command=?spawnable, "spawning command");
		let (proc, id) = if grouped {
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
		} else {
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
		};

		debug!("running post-spawn handler");
		Ok((
			proc,
			id,
			PostSpawn {
				command: command.clone(),
				events: actioned_events.clone(),
				id,
				grouped,
				supervisor_id,
			},
		))
	})?;

	post_spawn_handler
		.call(post_spawn)
		.await
		.map_err(|e| rte("action post-spawn", e.as_ref()))?;

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
		// generates pseudo-random u64 using [xorshift*](https://en.wikipedia.org/wiki/Xorshift#xorshift*)
		let mut seed = RandomState::new().build_hasher().finish();

		seed ^= seed >> 12;
		seed ^= seed << 25;
		seed ^= seed >> 27;

		let non_zero = seed.saturating_add(1);

		// Safety:
		// 1. The Saturating add ensures the value of `non_zero` is at least 1.
		unsafe { Self(NonZeroU64::new_unchecked(non_zero)) }
	}
}
