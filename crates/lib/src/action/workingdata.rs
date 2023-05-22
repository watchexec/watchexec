use std::{
	collections::HashMap,
	fmt,
	sync::{Arc, Weak},
	time::Duration,
};
use tokio::{
	process::Command as TokioCommand,
	sync::{Mutex, MutexGuard, OwnedMutexGuard},
};

use crate::{
	command::{Command, SupervisorId},
	event::Event,
	filter::Filterer,
	handler::HandlerLock,
};

use super::Outcome;

/// The configuration of the [action][crate::action] worker.
///
/// This is marked non-exhaustive so new configuration can be added without breaking.
#[derive(Clone)]
#[non_exhaustive]
pub struct WorkingData {
	/// How long to wait for events to build up before executing an action.
	///
	/// This is sometimes called "debouncing." We debounce on the trailing edge: an action is
	/// triggered only after that amount of time has passed since the first event in the cycle. The
	/// action is called with all the collected events in the cycle.
	pub throttle: Duration,

	/// The main handler to define: what to do when an action is triggered.
	///
	/// This handler is called with the [`Action`] environment, which has a certain way of returning
	/// the desired outcome, check out the [`Action::outcome()`] method. The handler checks for the
	/// outcome as soon as the handler returns, which means that if the handler returns before the
	/// outcome is set, you'll get unexpected results. For this reason, it's a bad idea to use ex. a
	/// channel as the handler.
	///
	/// If this handler is not provided, it defaults to a no-op, which does absolutely nothing, not
	/// even quit. Hence, you really need to provide a handler.
	///
	/// It is possible to change the handler or any other configuration inside the previous handler.
	/// It's useful to know that the handlers are updated from this working data before any of them
	/// run in any given cycle, so changing the pre-spawn and post-spawn handlers from this handler
	/// will not affect the running action.
	pub action_handler: HandlerLock<Action>,

	/// A handler triggered before a command is spawned.
	///
	/// This handler is called with the [`PreSpawn`] environment, which provides mutable access to
	/// the [`Command`](TokioCommand) which is about to be run. See the notes on the
	/// [`PreSpawn::command()`] method for important information on what you can do with it.
	///
	/// Returning an error from the handler will stop the action from processing further, and issue
	/// a [`RuntimeError`][crate::error::RuntimeError] to the error channel.
	pub pre_spawn_handler: HandlerLock<PreSpawn>,

	/// A handler triggered immediately after a command is spawned.
	///
	/// This handler is called with the [`PostSpawn`] environment, which provides details on the
	/// spawned command, including its PID.
	///
	/// Returning an error from the handler will drop the [`Child`][tokio::process::Child], which
	/// will terminate the command without triggering any of the normal Watchexec behaviour, and
	/// issue a [`RuntimeError`][crate::error::RuntimeError] to the error channel.
	pub post_spawn_handler: HandlerLock<PostSpawn>,

	/// Commands to execute.
	///
	/// These will be run in order, and an error will stop early.
	pub commands: Vec<Command>,

	/// Whether to use process groups (on Unix) or job control (on Windows) to run the command.
	///
	/// This makes use of [command_group] under the hood.
	///
	/// If you want to known whether a spawned command was run in a process group, you should use
	/// the value in [`PostSpawn`] instead of reading this one, as it may have changed in the
	/// meantime.
	pub grouped: bool,

	/// The filterer implementation to use when filtering events.
	///
	/// The default is a no-op, which will always pass every event.
	pub filterer: Arc<dyn Filterer>,
}

impl fmt::Debug for WorkingData {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("WorkingData")
			.field("throttle", &self.throttle)
			.field("commands", &self.commands)
			.field("grouped", &self.grouped)
			.field("filterer", &self.filterer)
			.finish_non_exhaustive()
	}
}

impl Default for WorkingData {
	fn default() -> Self {
		Self {
			throttle: Duration::from_millis(50),
			action_handler: Default::default(),
			pre_spawn_handler: Default::default(),
			post_spawn_handler: Default::default(),
			commands: Vec::new(),
			grouped: true,
			filterer: Arc::new(()),
		}
	}
}

/// The environment given to the action handler.
///
/// This deliberately does not implement Clone to make it hard to move it out of the handler, which
/// you should not do.
///
/// The [`Action::outcome()`] method is the only way to set the outcome of the action, and it _must_
/// be called before the handler returns.
pub struct Action {
	/// The collected events which triggered the action.
	pub events: Arc<[Event]>,
	processes: Arc<[SupervisorId]>,
	pub(crate) outcomes: Arc<Mutex<HashMap<SupervisorId, (Resolution, EventSet)>>>,
}

impl std::fmt::Debug for Action {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("Action")
			.field("events", &self.events)
			.field("processes", &self.processes)
			.finish()
	}
}

impl Action {
	pub(crate) fn new(events: Arc<[Event]>, processes: Arc<[SupervisorId]>) -> Self {
		Self {
			events,
			processes,
			outcomes: Default::default(),
		}
	}

	/// Set the action's outcome for all [`Command`]es.
	///
	/// This takes `self` and `Action` is not `Clone`, so it's only possible to call it once.
	/// Regardless, if you _do_ manage to call it twice, it will do nothing beyond the first call.
	///
	/// See the [`Action`] documentation about handlers to learn why it's a bad idea to clone or
	/// send it elsewhere, and what kind of handlers you cannot use.
	pub fn outcome(self, outcome: Outcome) {
		let mut outcomes = tokio::task::block_in_place(|| self.outcomes.blocking_lock());
		for process in self.processes.iter().copied().collect::<Vec<_>>() {
			self.on_command_with_lock(&mut outcomes, process, outcome.clone(), EventSet::All);
		}
	}

	/// Sets an [`Outcome`] for a single [`Command`].
	///
	/// The [`EventSet`] provides the specific set of [`Event`]s associated with the [`Command`]
	/// and [`Outcome`].
	pub async fn on_command(&self, process: SupervisorId, outcome: Outcome, set: EventSet) {
		let mut guard = self.outcomes.lock().await;
		self.on_command_with_lock(&mut guard, process, outcome, set);
	}

	// Used internally by on_command and outcome to insert into `outcomes`.
	fn on_command_with_lock(
		&self,
		guard: &mut MutexGuard<'_, HashMap<SupervisorId, (Resolution, EventSet)>>,
		process: SupervisorId,
		outcome: Outcome,
		set: EventSet,
	) {
		guard.insert(process, (Resolution::Apply(outcome), set));
	}

	/// Returns a snapshot of the [`SupervisorId`]s of the running [`Command`]s at creation of the
	/// [`Action`].
	pub fn current_processes(&self) -> &[SupervisorId] {
		&self.processes
	}

	/// Starts a new supervised [`Command`].
	///
	/// This instantiates a new command supervisor, which manages the lifecycle
	/// of a command within the watchexec instance, including stopping, starting
	/// again, sending signals, and monitoring its aliveness.
	///
	/// You can control it by calling `command_outcome()` with the [`SupervisorId`]
	/// this method returns. To destroy a supervisor, use `Outcome::End`.
	///
	/// For details on the `events` argument, see the documentation for
	/// `command_outcome`.
	///
	/// Note that as this is async, your action handler must also be async. Calling
	/// this method in a sync handler without `await`ing it will do nothing.
	pub async fn start_process(&self, cmd: Command, set: EventSet) -> SupervisorId {
		let process = SupervisorId::default();
		let mut processes = self.outcomes.lock().await;
		processes.insert(process, (Resolution::Start(cmd), set));

		process
	}
}

/// Indicates how a `Command` should be resolved.
#[derive(Debug, Clone)]
pub enum Resolution {
	/// Used to start a new `Command` with a `Command`.
	Start(Command),
	/// Apply an `Outcome` to an existing `Command`.
	Apply(Outcome),
}

/// Specifies whether to use all `Event`s, a subset, or none at all.
#[derive(Debug, Clone)]
pub enum EventSet {
	/// All `Event`s associated with an action.
	All,
	/// A select subset of `Event`s
	Some(Vec<Event>),
	/// No `Event`s at all.
	None,
}

/// The environment given to the pre-spawn handler.
///
/// This deliberately does not implement Clone to make it hard to move it out of the handler, which
/// you should not do.
///
/// The [`PreSpawn::command()`] method is the only way to mutate the command, and the mutex guard it
/// returns _must_ be dropped before the handler returns.
#[derive(Debug)]
#[non_exhaustive]
pub struct PreSpawn {
	/// The command which is about to be spawned.
	pub command: Command,

	/// The collected events which triggered the action this command issues from.
	pub events: Arc<[Event]>,

	/// The
	supervisor_id: SupervisorId,

	to_spawn_w: Weak<Mutex<TokioCommand>>,
}

impl PreSpawn {
	pub(crate) fn new(
		command: Command,
		to_spawn: TokioCommand,
		events: Arc<[Event]>,
		supervisor_id: SupervisorId,
	) -> (Self, Arc<Mutex<TokioCommand>>) {
		let arc = Arc::new(Mutex::new(to_spawn));
		(
			Self {
				command,
				events,
				supervisor_id,
				to_spawn_w: Arc::downgrade(&arc),
			},
			arc.clone(),
		)
	}

	/// Get write access to the command that will be spawned.
	///
	/// Keeping the lock alive beyond the end of the handler may cause the command to be cancelled,
	/// but note no guarantees are made on this behaviour. Just don't do it. See the [`Action`]
	/// documentation about handlers for more.
	///
	/// This will always return `Some()` under normal circumstances.
	pub async fn command(&self) -> Option<OwnedMutexGuard<TokioCommand>> {
		if let Some(arc) = self.to_spawn_w.upgrade() {
			Some(arc.lock_owned().await)
		} else {
			None
		}
	}

	/// Returns the `SupervisorId` associated with the `Supervisor` and `Command`.
	pub fn process(&self) -> SupervisorId {
		self.supervisor_id
	}
}

/// The environment given to the post-spawn handler.
///
/// This is Clone, as there's nothing (except returning an error) that can be done to the command
/// now that it's spawned, as far as Watchexec is concerned. Nevertheless, you should return from
/// this handler quickly, to avoid holding up anything else.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct PostSpawn {
	/// The command the process was spawned with.
	pub command: Command,

	/// The collected events which triggered the action the command issues from.
	pub events: Arc<[Event]>,

	/// The process ID or the process group ID.
	pub id: u32,

	/// Whether the command was run in a process group.
	pub grouped: bool,

	/// The `SupervisorId` associated with the process' `Supervisor`.
	pub(crate) supervisor_id: SupervisorId,
}

impl PostSpawn {
	/// Returns the `SupervisorId` associated with the `Supervisor` and the `Command` that was run.
	pub fn process(&self) -> SupervisorId {
		self.supervisor_id
	}
}
