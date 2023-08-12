use std::{
	collections::HashMap,
	fmt,
	path::Path,
	sync::Mutex,
	sync::{Arc, Weak},
	time::Duration,
};
use tokio::{
	process::Command as TokioCommand,
	sync::{Mutex as TokioMutex, OwnedMutexGuard},
};
use watchexec_events::{FileType, ProcessEnd};
use watchexec_signals::Signal;

use crate::{
	action::Outcome,
	command::{Command, Isolation, Program, SupervisorId},
	event::Event,
	filter::Filterer,
	handler::HandlerLock,
};

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
	/// This handler is called with the [`Action`] environment, look at its doc for more detail.
	///
	/// Watchexec waits until the handler is done, and then performs any actions the handler
	/// told it to. "Doneness" is determined by the handler returning, or resolving in case of
	/// an async handler. You'll get unexpected results using eg a channel as the handler, as
	/// the handler implementation will immediately return after sending to the channel, and
	/// act as a no-op.
	///
	/// If this handler is not provided, or does nothing, Watchexec in turn will do nothing, not
	/// even quit. Hence, you really need to provide a handler.
	///
	/// It is possible to change the handler or any other configuration inside the previous handler.
	/// It's useful to know that the handlers are updated from this working data before any of them
	/// run in any given cycle, so changing the pre-spawn and post-spawn handlers from this handler
	/// will not affect the running action.
	pub action_handler: HandlerLock<Action>,

	// TODO: spawn handlers should really go inside Outcome or Create, not be defined here
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

	/// The filterer implementation to use when filtering events.
	///
	/// The default is a no-op, which will always pass every event.
	pub filterer: Arc<dyn Filterer>,
}

impl fmt::Debug for WorkingData {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("WorkingData")
			.field("throttle", &self.throttle)
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
			filterer: Arc::new(()),
		}
	}
}

/// The environment given to the action handler.
///
/// This deliberately does not implement Clone to make it hard to move it out of the handler, which
/// you should not do.
///
/// The action handler is the heart of a Watchexec program. Within, you decide what happens when an
/// event successfully passes all filters. Watchexec maintains a set of Supervised Commands, which
/// are assigned a SupervisorId for lightweight reference. In this action handler, you should
/// add commands to be supervised with `create()`, apply [`Outcome`]s to them when they need to
/// change with `apply()`, and `delete()` them when they're not longer needed. While you're
/// encouraged to keep track of the Supervised Commands yourself, the `list()` method also lets you
/// query what commands are currently known to Watchexec.
///
/// Each method that handles supervised commands takes an [`EventSet`] argument, which is used to
/// describe which events led to an action being taken on which command. `EventSet::All` should be
/// the default if you're not sure what to do. This set of events is passed to the `PreSpawn` and
/// `PostSpawn` handlers if they are called in response to an action.
///
/// It is important to note that methods called in this handler do not act immediately: rather they
/// build up a list of desired effects which will be applied when the handler returns.
pub struct Action {
	/// The collected events which triggered the action.
	pub events: Arc<[Event]>,

	/// A snapshot of the available set of Supervised Command IDs.
	///
	/// This is not a live list: if the actual set of supervised commands changes during the
	/// execution of this action, this will not be reflected here. However, that is not generally a
	/// problem: if an effect is queued through an action handler to apply to a supervised command
	/// that no longer exists when the handler returns, it is silently ignored.
	pub supervisors: Arc<[SupervisorId]>,
	// TODO: provide more info in the snapshot ie "is it running" etc
	pub(crate) supervision: Arc<Mutex<HashMap<SupervisorId, Vec<SupervisionOrder>>>>,
	pub(crate) instance: Arc<Mutex<Vec<InstanceOrder>>>,
}

impl std::fmt::Debug for Action {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("Action")
			.field("events", &self.events)
			.field("supervisors", &self.supervisors)
			.finish_non_exhaustive()
	}
}

impl Action {
	pub(crate) fn new(events: Arc<[Event]>, supervisors: Arc<[SupervisorId]>) -> Self {
		Self {
			events,
			supervisors,
			supervision: Default::default(),
			instance: Default::default(),
		}
	}

	/// Sets an [`Outcome`] for a single Supervised [`Command`].
	pub fn apply(&self, to: SupervisorId, outcome: Outcome, because_of: EventSet) {
		let mut orders = self.supervision.lock().expect("lock poisoned");
		orders
			.entry(to)
			.or_default()
			.push(SupervisionOrder::Apply(outcome.clone(), because_of));
	}

	/// Creates a new Supervised [`Command`].
	///
	/// This does not _start_ the command. To do so, call `apply()` immediately after this with an
	/// `Outcome::Start`.
	///
	/// Returns an opaque ID to use to later `apply()` outcomes to this supervised command.
	pub fn create(&self, command: Command) -> SupervisorId {
		let id = SupervisorId::default();
		let mut orders = self.supervision.lock().expect("lock poisoned");
		orders
			.entry(id)
			.or_default()
			.push(SupervisionOrder::Create(command));
		id
	}

	/// Removes an alive [`Command`] for this and all the following [`Action`]s.
	///
	/// This implies applying an [`Outcome::Stop`]. The supervised command is killed if it was alive,
	/// then removed from the Watchexec instance. To start the command again, `create()` must be
	/// called again.
	///
	/// To gracefully stop a supervised command instead, call `apply()` with the relevant `Outcome`
	/// _before_ calling this.
	pub fn remove(&self, id: SupervisorId) {
		let mut orders = self.supervision.lock().expect("lock poisoned");
		orders.entry(id).or_default().push(SupervisionOrder::Remove);
	}

	/// Stops all supervised commands and then shuts down the Watchexec instance.
	///
	/// If a graceful stop is required, use `apply()` beforehand on all commands.
	pub fn quit(&self) {
		self.instance
			.lock()
			.expect("lock poisoned")
			.push(InstanceOrder::Quit);
	}

	/// Convenience to get all signals in the event set.
	pub fn signals(&self) -> impl Iterator<Item = Signal> + '_ {
		self.events.iter().flat_map(Event::signals)
	}

	/// TODO: proper doc
	///
	/// an action contains a set of events, and some of those events might relate to watched
	/// files, and each of *those* events may have one or more paths that were affected.
	/// to hide this complexity this method just provides any and all paths in the event,
	/// along with the type of file at that path, if watchexec knows that
	pub fn paths(&self) -> impl Iterator<Item = (&Path, Option<&FileType>)> + '_ {
		self.events.iter().flat_map(Event::paths)
	}

	/// Convenience to get all process completions in the event set.
	pub fn completions(&self) -> impl Iterator<Item = Option<ProcessEnd>> + '_ {
		self.events.iter().flat_map(Event::completions)
	}
}

/// Orders a Watchexec instance applies to the supervision set.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SupervisionOrder {
	/// Create a new supervised command.
	Create(Command),

	/// Apply an [`Outcome`] to an existing supervised command in response to some events.
	Apply(Outcome, EventSet),

	/// Stop and remove a supervised command.
	Remove,
}

/// Orders a Watchexec instance applies to itself.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum InstanceOrder {
	/// Stop all supervised commands and then quit.
	Quit,
}

/// Specifies whether to use all `Event`s, a subset, or none at all.
#[derive(Debug, Clone, Default)]
pub enum EventSet {
	/// All `Event`s associated with an action.
	#[default]
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
	/// The program which is about to be spawned.
	pub program: Program,

	/// Isolation method the program is run in.
	pub isolation: Isolation,

	/// The collected events which triggered the action this command issues from.
	pub events: Arc<[Event]>,

	supervisor_id: SupervisorId,

	to_spawn_w: Weak<TokioMutex<TokioCommand>>,
}

impl PreSpawn {
	pub(crate) fn new(
		program: Program,
		isolation: Isolation,
		to_spawn: TokioCommand,
		events: Arc<[Event]>,
		supervisor_id: SupervisorId,
	) -> (Self, Arc<TokioMutex<TokioCommand>>) {
		let arc = Arc::new(TokioMutex::new(to_spawn));
		(
			Self {
				program,
				isolation,
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
	pub const fn supervisor(&self) -> SupervisorId {
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
	/// The program the process was spawned with.
	pub program: Program,

	/// The collected events which triggered the action the command issues from.
	pub events: Arc<[Event]>,

	/// The process ID or the process group ID.
	pub id: u32,

	/// Isolation method the program is run in.
	pub isolation: Isolation,

	/// The `SupervisorId` associated with the process' `Supervisor`.
	pub(crate) supervisor_id: SupervisorId,
}

impl PostSpawn {
	/// Returns the `SupervisorId` associated with the `Supervisor` and the `Command` that was run.
	pub const fn supervisor(&self) -> SupervisorId {
		self.supervisor_id
	}
}
