use std::{
	fmt,
	sync::{Arc, Weak},
	time::Duration,
};

use atomic_take::AtomicTake;
use once_cell::sync::OnceCell;
use tokio::{
	process::Command,
	sync::{Mutex, OwnedMutexGuard},
};

pub use command_group::Signal;

use crate::{command::Shell, event::Event, filter::Filterer, handler::Handler};

use super::Outcome;

#[derive(Clone)]
#[non_exhaustive]
pub struct WorkingData {
	pub throttle: Duration,

	pub action_handler: Arc<AtomicTake<Box<dyn Handler<Action> + Send>>>,
	pub pre_spawn_handler: Arc<AtomicTake<Box<dyn Handler<PreSpawn> + Send>>>,
	pub post_spawn_handler: Arc<AtomicTake<Box<dyn Handler<PostSpawn> + Send>>>,

	/// TODO: notes for command construction ref Shell and old src
	pub command: Vec<String>,
	pub grouped: bool,
	pub shell: Shell,

	pub filterer: Arc<dyn Filterer>,
}

impl fmt::Debug for WorkingData {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("WorkingData")
			.field("throttle", &self.throttle)
			.field("shell", &self.shell)
			.field("command", &self.command)
			.field("grouped", &self.grouped)
			.field("filterer", &self.filterer)
			.finish_non_exhaustive()
	}
}

impl Default for WorkingData {
	fn default() -> Self {
		Self {
			// set to 50ms here, but will remain 100ms on cli until 2022
			throttle: Duration::from_millis(50),
			action_handler: Arc::new(AtomicTake::new(Box::new(()) as _)),
			pre_spawn_handler: Arc::new(AtomicTake::new(Box::new(()) as _)),
			post_spawn_handler: Arc::new(AtomicTake::new(Box::new(()) as _)),
			command: Vec::new(),
			shell: Shell::default(),
			grouped: true,
			filterer: Arc::new(()),
		}
	}
}

#[derive(Debug, Default)]
pub struct Action {
	pub events: Vec<Event>,
	pub(super) outcome: Arc<OnceCell<Outcome>>,
}

impl Action {
	pub(super) fn new(events: Vec<Event>) -> Self {
		Self {
			events,
			..Self::default()
		}
	}

	/// Set the action's outcome.
	///
	/// This takes `self` and `Action` is not `Clone`, so it's only possible to call it once.
	/// Regardless, if you _do_ manage to call it twice, it will do nothing beyond the first call.
	///
	/// See the [`Action`] documentation about handlers to learn why it's a bad idea to clone or
	/// send it elsewhere, and what kind of handlers you cannot use.
	pub fn outcome(self, outcome: Outcome) {
		self.outcome.set(outcome).ok();
	}
}

#[derive(Debug)]
#[non_exhaustive]
pub struct PreSpawn {
	pub command: Vec<String>,
	command_w: Weak<Mutex<Command>>,
}

impl PreSpawn {
	pub(super) fn new(command: Command, cmd: Vec<String>) -> (Self, Arc<Mutex<Command>>) {
		let arc = Arc::new(Mutex::new(command));
		(
			Self {
				command: cmd,
				command_w: Arc::downgrade(&arc),
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
	pub async fn command(&self) -> Option<OwnedMutexGuard<Command>> {
		if let Some(arc) = self.command_w.upgrade() {
			Some(arc.lock_owned().await)
		} else {
			None
		}
	}
}

#[derive(Debug)]
#[non_exhaustive]
pub struct PostSpawn {
	pub command: Vec<String>,
	pub id: u32,
	pub grouped: bool,
}
