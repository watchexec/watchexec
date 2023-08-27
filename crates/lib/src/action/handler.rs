use std::{
	collections::HashMap,
	fmt,
	path::Path,
	sync::{Arc, Mutex, MutexGuard},
};
use tokio::process::Command as TokioCommand;
use watchexec_events::{Event, FileType, ProcessEnd};
use watchexec_signals::Signal;

use crate::{
	action::Outcome,
	command::{Command, Isolation, Program, SupervisorId},
};

/// The environment given to the action handler.
///
/// This deliberately does not implement Clone to make it hard to move it out of the handler, which
/// you should not do.
///
/// The action handler is the heart of a Watchexec program. Within, you decide what happens when an
/// event successfully passes all filters. Watchexec maintains a set of Supervised Commands, which
/// are assigned a [`SupervisorId`] for lightweight reference. In this action handler, you should
/// add commands to be supervised with `create()`, apply [`Outcome`]s to them when they need to
/// change with `apply()`, and `delete()` them when they're not longer needed. While you're
/// encouraged to keep track of the Supervised Commands yourself, the `list()` method also lets you
/// query what commands are currently known to Watchexec.
///
/// Each method that handles supervised commands takes an [`EventSet`] argument, which is used to
/// describe which events led to an action being taken on which command. `EventSet::All` should be
/// the default if you're not sure what to do. This set of events is passed to the `PreSpawn`
/// handler if it is called in response to an action.
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

	/// Clears the outcome queue for a supervisor.
	///
	/// When the handler returns, this clears any existing outcome queue on the supervisor. That
	/// is, if the supervisor is currently applying a graceful stop sequence like:
	///
	/// ```
	/// # use std::time::Duration;
	/// # use watchexec::action::Outcome;
	/// # use watchexec::action::Outcome::*;
	/// # use watchexec_signals::Signal::*;
	/// # let outcome =
	/// Outcome::both(
	///     Signal(Interrupt),
	///     Outcome::wait_timeout(
	///         Duration::from_secs(30),
	///         Outcome::both(Stop, Wait),
	///     )
	/// )
	/// # ;
	/// ```
	///
	/// and is currently waiting, a `clear()` will prevent `Both(Stop, Wait)` from running and also
	/// interrupt the wait/timeout.
	///
	/// This is most useful in conjunction with `apply()`. Any `apply()` calls before `clear()`
	/// will be erased.
	pub fn clear(&self, sid: SupervisorId) {
		use SupervisionOrder::*;
		let mut orders = self.supervision.lock().expect("lock poisoned");
		let orders = orders.entry(sid).or_default();

		// a Destroy prevents more actions
		if orders.contains(&Destroy) {
			return;
		}

		// clearing right after a create doesn't do anything
		if let Some(Create(_)) = orders.last() {
			return;
		}

		// discard all previous clears and applys
		orders.retain(|o| !matches!(o, Clear | Apply(_, _)));

		orders.push(Clear);
	}

	/// Adds an [`Outcome`] to a supervisor.
	///
	/// This joins the given outcome to the queue of outcomes currently applying to the supervisor,
	/// if any, or starts applying it if there's none. Note that this happens once the action
	/// handler returns, not immediately.
	///
	/// If `apply()` has been called onto this supervisor before within this run of the handler,
	/// the outcomes are combined (ie the equivalent of calling it once with `Outcome::sequence()`
	/// and the arguments of the first to last calls).
	pub fn apply(&self, sid: SupervisorId, outcome: Outcome, because_of: EventSet) {
		use SupervisionOrder::*;
		let mut orders = self.supervision.lock().expect("lock poisoned");
		let orders = orders.entry(sid).or_default();

		// a Destroy prevents more actions
		if orders.contains(&Destroy) {
			return;
		}

		// if the last order is an apply, combine it on the spot
		if let Some(apply @ Apply(_, _)) = orders.last_mut() {
			apply.combine_apply(Apply(outcome, because_of));
		} else {
			orders.push(Apply(outcome, because_of));
		}
	}

	/// Creates a new Supervised [`Command`].
	///
	/// This does not _start_ the command. To do so, call `apply()` immediately after this with an
	/// `Outcome::Start`.
	///
	/// Returns an opaque ID to use to later `apply()` outcomes to this supervised command.
	pub fn create(&self, command: Command) -> SupervisorId {
		let sid = SupervisorId::default();
		let mut orders = self.supervision.lock().expect("lock poisoned");

		// in the unlikely event there's a collision, retry
		if orders.contains_key(&sid) || self.supervisors.contains(&sid) {
			return self.create(command);
		}

		orders.insert(sid, vec![SupervisionOrder::Create(command)]);
		sid
	}

	/// Destroys a supervisor.
	///
	/// This waits until the supervisor's outcome queue is clear, then kills the command if it's still
	/// alive, and removes the supervisor from the Watchexec instance. To start the command again,
	/// `create()` must be called again.
	///
	/// To gracefully stop a supervised command instead, call `apply()` with the relevant `Outcome`
	/// _before_ calling this.
	///
	/// To skip waiting for the outcome queue to clear, call `clear()` before this.
	///
	/// Anything applied after this is ignored.
	pub fn destroy(&self, sid: SupervisorId) {
		let mut orders = self.supervision.lock().expect("lock poisoned");
		orders
			.entry(sid)
			.or_default()
			.push(SupervisionOrder::Destroy);
	}

	/// Shuts down the Watchexec instance.
	///
	/// If a more graceful stop is required, use `apply()` beforehand on all commands.
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

	/// Convenience to get all paths in the event set.
	///
	/// An action contains a set of events, and some of those events might relate to watched
	/// files, and each of *those* events may have one or more paths that were affected.
	/// To hide this complexity this method just provides any and all paths in the event,
	/// along with the type of file at that path, if Watchexec knows that.
	pub fn paths(&self) -> impl Iterator<Item = (&Path, Option<&FileType>)> + '_ {
		self.events.iter().flat_map(Event::paths)
	}

	/// Convenience to get all process completions in the event set.
	pub fn completions(&self) -> impl Iterator<Item = Option<ProcessEnd>> + '_ {
		self.events.iter().flat_map(Event::completions)
	}
}

/// Orders a Watchexec instance applies to the supervision set.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum SupervisionOrder {
	/// Create a new supervised command.
	Create(Command),

	/// Clear the outcome queue of a supervisor.
	Clear,

	/// Apply an [`Outcome`] to a supervisor in response to some events.
	Apply(Outcome, EventSet),

	/// Stop and destroy a supervisor.
	Destroy,
}

impl SupervisionOrder {
	pub(crate) fn combine_apply(&mut self, other: Self) {
		let Self::Apply(prior_outcome, prior_event_set) = self else {
			panic!("combine_apply() called without an Apply");
		};
		let Self::Apply(newer_outcome, newer_event_set) = other else {
			panic!("combine_apply() called without an Apply");
		};

		*self = Self::Apply(
			Outcome::both(prior_outcome.clone(), newer_outcome),
			match (prior_event_set.clone(), newer_event_set) {
				(EventSet::None, set) | (set, EventSet::None) => set,
				(EventSet::All, _) | (_, EventSet::All) => EventSet::All,
				(EventSet::Some(mut one), EventSet::Some(two)) => {
					one.extend(two);
					EventSet::Some(one)
				}
			},
		);
	}
}

/// Orders a Watchexec instance applies to itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum InstanceOrder {
	/// Stop all supervised commands and then quit.
	Quit,
}

/// Specifies whether to use all `Event`s, a subset, or none at all.
#[derive(Clone, Default, Debug, Eq, PartialEq)]
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

	command: Arc<Mutex<TokioCommand>>,
}

impl PreSpawn {
	pub(crate) fn new(
		program: Program,
		isolation: Isolation,
		command: TokioCommand,
		events: Arc<[Event]>,
		supervisor_id: SupervisorId,
	) -> (Self, Arc<Mutex<TokioCommand>>) {
		let command = Arc::new(Mutex::new(command));
		(
			Self {
				program,
				isolation,
				events,
				supervisor_id,
				command: command.clone(),
			},
			command,
		)
	}

	/// Get write access to the command that will be spawned.
	///
	/// Keeping the lock alive beyond the end of the handler will cause a panic.
	///
	/// # Panics
	/// Panics if the inner lock is poisoned or the command is not available.
	pub fn command(&self) -> MutexGuard<'_, TokioCommand> {
		self.command.lock().expect("prespawn lock poisoned")
	}

	/// Returns the `SupervisorId` associated with the `Supervisor` and `Command`.
	pub const fn supervisor(&self) -> SupervisorId {
		self.supervisor_id
	}
}
