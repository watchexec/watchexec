use std::{
	fmt,
	sync::{Arc, Weak},
	time::Duration,
};

use once_cell::sync::OnceCell;
use tokio::{
	process::Command,
	sync::{Mutex, OwnedMutexGuard},
};

use crate::{command::Shell, event::Event, filter::Filterer, handler::HandlerLock};

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
	/// the [`Command`] which is about to be run. See the notes on the [`PreSpawn::command()`]
	/// method for important information on what you can do with it.
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

	/// Command to execute.
	///
	/// When `shell` is [`Shell::None`], this is expected to be in “execvp(3)” format: first
	/// program, rest arguments. Otherwise, all elements will be joined together with a single space
	/// and passed to the shell. More control can then be obtained by providing a 1-element vec, and
	/// doing your own joining and/or escaping there.
	pub command: Vec<String>,

	/// Whether to use process groups (on Unix) or job control (on Windows) to run the command.
	///
	/// This makes use of [command_group] under the hood.
	///
	/// If you want to known whether a spawned command was run in a process group, you should use
	/// the value in [`PostSpawn`] instead of reading this one, as it may have changed in the
	/// meantime.
	pub grouped: bool,

	/// The shell to use to run the command.
	///
	/// See the [`Shell`] enum documentation for more details.
	pub shell: Shell,

	/// The filterer implementation to use when filtering events.
	///
	/// The default is a no-op, which will always pass every event.
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
			action_handler: Default::default(),
			pre_spawn_handler: Default::default(),
			post_spawn_handler: Default::default(),
			command: Vec::new(),
			shell: Shell::default(),
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
#[derive(Debug)]
pub struct Action {
	/// The collected events which triggered the action.
	pub events: Arc<[Event]>,
	pub(super) outcome: Arc<OnceCell<Outcome>>,
}

impl Action {
	pub(super) fn new(events: Arc<[Event]>) -> Self {
		Self {
			events,
			outcome: Default::default(),
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
	///
	/// This is the final command, after the [`Shell`] has been applied.
	pub command: Vec<String>,

	/// The collected events which triggered the action this command issues from.
	pub events: Arc<[Event]>,

	command_w: Weak<Mutex<Command>>,
}

impl PreSpawn {
	pub(super) fn new(
		command: Command,
		cmd: Vec<String>,
		events: Arc<[Event]>,
	) -> (Self, Arc<Mutex<Command>>) {
		let arc = Arc::new(Mutex::new(command));
		(
			Self {
				command: cmd,
				events,
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

/// The environment given to the post-spawn handler.
///
/// This is Clone, as there's nothing (except returning an error) that can be done to the command
/// now that it's spawned, as far as Watchexec is concerned. Nevertheless, you should return from
/// this handler quickly, to avoid holding up anything else.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct PostSpawn {
	/// The final command the process was spawned with.
	pub command: Vec<String>,

	/// The collected events which triggered the action the command issues from.
	pub events: Arc<[Event]>,

	/// The process ID or the process group ID.
	pub id: u32,

	/// Whether the command was run in a process group.
	pub grouped: bool,
}
