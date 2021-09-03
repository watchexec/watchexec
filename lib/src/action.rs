//! Processor responsible for receiving events, filtering them, and scheduling actions in response.

use std::{
	fmt,
	sync::{Arc, Weak},
	time::{Duration, Instant},
};

use atomic_take::AtomicTake;
use clearscreen::ClearScreen;
use once_cell::sync::OnceCell;
use tokio::{
	process::Command,
	sync::{mpsc, watch, Mutex, OwnedMutexGuard},
	time::timeout,
};
use tracing::{debug, trace, warn};

use crate::{
	command::{Shell, Supervisor},
	error::{CriticalError, RuntimeError},
	event::Event,
	handler::{rte, Handler},
};

pub use command_group::Signal;

#[derive(Clone)]
#[non_exhaustive]
pub struct WorkingData {
	pub throttle: Duration,

	pub action_handler: Arc<AtomicTake<Box<dyn Handler<Action> + Send>>>,
	pub pre_spawn_handler: Arc<AtomicTake<Box<dyn Handler<PreSpawn> + Send>>>,
	pub post_spawn_handler: Arc<AtomicTake<Box<dyn Handler<PostSpawn> + Send>>>,

	pub shell: Shell,

	/// TODO: notes for command construction ref Shell and old src
	pub command: Vec<String>,

	pub grouped: bool,
}

impl fmt::Debug for WorkingData {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("WorkingData")
			.field("throttle", &self.throttle)
			.field("shell", &self.shell)
			.field("command", &self.command)
			.field("grouped", &self.grouped)
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
			shell: Shell::default(),
			command: Vec::new(),
			grouped: true,
		}
	}
}

#[derive(Debug, Default)]
pub struct Action {
	pub events: Vec<Event>,
	outcome: Arc<OnceCell<Outcome>>,
}

impl Action {
	fn new(events: Vec<Event>) -> Self {
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
	fn new(command: Command, cmd: Vec<String>) -> (Self, Arc<Mutex<Command>>) {
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

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Outcome {
	/// Stop processing this action silently.
	DoNothing,

	/// If the command is running, stop it.
	Stop,

	/// If the command isn't running, start it.
	Start,

	/// Wait for command completion.
	Wait,

	/// Send this signal to the command.
	Signal(Signal),

	/// Clear the (terminal) screen.
	Clear,

	/// Reset the (terminal) screen.
	///
	/// This invokes: [`WindowsCooked`][ClearScreen::WindowsCooked],
	/// [`WindowsVt`][ClearScreen::WindowsVt], [`VtWellDone`][ClearScreen::VtWellDone],
	/// and [the default][ClearScreen::default()], in this order.
	Reset,

	/// Exit watchexec.
	Exit,

	/// When command is running, do the first, otherwise the second.
	IfRunning(Box<Outcome>, Box<Outcome>),

	/// Do both outcomes in order.
	Both(Box<Outcome>, Box<Outcome>),
}

impl Default for Outcome {
	fn default() -> Self {
		Self::DoNothing
	}
}

impl Outcome {
	pub fn if_running(then: Outcome, otherwise: Outcome) -> Self {
		Self::IfRunning(Box::new(then), Box::new(otherwise))
	}

	pub fn both(one: Outcome, two: Outcome) -> Self {
		Self::Both(Box::new(one), Box::new(two))
	}

	pub fn wait(and_then: Outcome) -> Self {
		Self::Both(Box::new(Outcome::Wait), Box::new(and_then))
	}

	fn resolve(self, is_running: bool) -> Self {
		match (is_running, self) {
			(true, Self::IfRunning(then, _)) => then.resolve(true),
			(false, Self::IfRunning(_, otherwise)) => otherwise.resolve(false),
			(ir, Self::Both(one, two)) => Self::both(one.resolve(ir), two.resolve(ir)),
			(_, other) => other,
		}
	}
}

pub async fn worker(
	working: watch::Receiver<WorkingData>,
	errors: mpsc::Sender<RuntimeError>,
	events_tx: mpsc::Sender<Event>,
	mut events: mpsc::Receiver<Event>,
) -> Result<(), CriticalError> {
	let mut last = Instant::now();
	let mut set = Vec::new();
	let mut process: Option<Supervisor> = None;

	let mut action_handler =
		{ working.borrow().action_handler.take() }.ok_or(CriticalError::MissingHandler)?;
	let mut pre_spawn_handler =
		{ working.borrow().pre_spawn_handler.take() }.ok_or(CriticalError::MissingHandler)?;
	let mut post_spawn_handler =
		{ working.borrow().post_spawn_handler.take() }.ok_or(CriticalError::MissingHandler)?;

	loop {
		let maxtime = if set.is_empty() {
			trace!("nothing in set, waiting forever for next event");
			Duration::from_secs(u64::MAX)
		} else {
			working.borrow().throttle.saturating_sub(last.elapsed())
		};

		if maxtime.is_zero() {
			if set.is_empty() {
				trace!("out of throttle but nothing to do, resetting");
				last = Instant::now();
				continue;
			} else {
				trace!("out of throttle on recycle");
			}
		} else {
			trace!(?maxtime, "waiting for event");
			match timeout(maxtime, events.recv()).await {
				Err(_timeout) => {
					trace!("timed out, cycling");
					continue;
				}
				Ok(None) => break,
				Ok(Some(event)) => {
					trace!(?event, "got event");

					if set.is_empty() {
						trace!("event is the first, resetting throttle window");
						last = Instant::now();
					}

					set.push(event);

					let elapsed = last.elapsed();
					if elapsed < working.borrow().throttle {
						trace!(?elapsed, "still within throttle window, cycling");
						continue;
					}
				}
			}
		}

		trace!("out of throttle, starting action process");
		last = Instant::now();

		let action = Action::new(set.drain(..).collect());
		debug!(?action, "action constructed");

		if let Some(h) = working.borrow().action_handler.take() {
			trace!("action handler updated");
			action_handler = h;
		}

		if let Some(h) = working.borrow().pre_spawn_handler.take() {
			trace!("pre-spawn handler updated");
			pre_spawn_handler = h;
		}

		if let Some(h) = working.borrow().post_spawn_handler.take() {
			trace!("post-spawn handler updated");
			post_spawn_handler = h;
		}

		debug!("running action handler");
		let outcome = action.outcome.clone();
		let err = action_handler
			.handle(action)
			.map_err(|e| rte("action worker", e));
		if let Err(err) = err {
			errors.send(err).await?;
			debug!("action handler errored, skipping");
			continue;
		}

		let outcome = outcome.get().cloned().unwrap_or_default();
		debug!(?outcome, "handler finished");

		let is_running = process.as_ref().map(|p| p.is_running()).unwrap_or(false);
		let outcome = outcome.resolve(is_running);
		debug!(?outcome, "outcome resolved");

		let w = working.borrow().clone();
		let rerr = apply_outcome(
			outcome,
			w,
			&mut process,
			&mut pre_spawn_handler,
			&mut post_spawn_handler,
			errors.clone(),
			events_tx.clone(),
		)
		.await;
		if let Err(err) = rerr {
			errors.send(err).await?;
		}
	}

	debug!("action worker finished");
	Ok(())
}

#[async_recursion::async_recursion]
async fn apply_outcome(
	outcome: Outcome,
	working: WorkingData,
	process: &mut Option<Supervisor>,
	pre_spawn_handler: &mut Box<dyn Handler<PreSpawn> + Send>,
	post_spawn_handler: &mut Box<dyn Handler<PostSpawn> + Send>,
	errors: mpsc::Sender<RuntimeError>,
	events: mpsc::Sender<Event>,
) -> Result<(), RuntimeError> {
	trace!(?outcome, "applying outcome");
	match (process.as_mut(), outcome) {
		(_, Outcome::DoNothing) => {}
		(_, Outcome::Exit) => {
			return Err(RuntimeError::Exit);
		}
		(Some(p), Outcome::Stop) => {
			p.kill().await;
			p.wait().await?;
			*process = None;
		}
		(None, o @ Outcome::Stop) | (None, o @ Outcome::Wait) | (None, o @ Outcome::Signal(_)) => {
			debug!(outcome=?o, "meaningless without a process, not doing anything");
		}
		(_, Outcome::Start) => {
			if working.command.is_empty() {
				warn!("tried to start a command without anything to run");
			} else {
				let command = working.shell.to_command(&working.command);
				let (pre_spawn, command) = PreSpawn::new(command, working.command.clone());

				debug!("running pre-spawn handler");
				pre_spawn_handler
					.handle(pre_spawn)
					.map_err(|e| rte("action pre-spawn", e))?;

				let mut command = Arc::try_unwrap(command)
					.map_err(|_| RuntimeError::HandlerLockHeld("pre-spawn"))?
					.into_inner();

				trace!("spawing supervisor for command");
				let sup = Supervisor::spawn(
					errors.clone(),
					events.clone(),
					&mut command,
					working.grouped,
				)?;

				debug!("running post-spawn handler");
				let post_spawn = PostSpawn {
					command: working.command.clone(),
					id: sup.id(),
					grouped: working.grouped,
				};
				post_spawn_handler
					.handle(post_spawn)
					.map_err(|e| rte("action post-spawn", e))?;

				// TODO: consider what we want to do for processes still running here?
				*process = Some(sup);
			}
		}

		(Some(p), Outcome::Signal(sig)) => {
			// TODO: windows
			p.signal(sig).await;
		}

		(Some(p), Outcome::Wait) => {
			p.wait().await?;
		}

		(_, Outcome::Clear) => {
			clearscreen::clear()?;
		}

		(_, Outcome::Reset) => {
			for cs in [
				ClearScreen::WindowsCooked,
				ClearScreen::WindowsVt,
				ClearScreen::VtWellDone,
				ClearScreen::default(),
			] {
				cs.clear()?;
			}
		}

		(Some(_), Outcome::IfRunning(then, _)) => {
			apply_outcome(
				*then,
				working,
				process,
				pre_spawn_handler,
				post_spawn_handler,
				errors,
				events,
			)
			.await?;
		}
		(None, Outcome::IfRunning(_, otherwise)) => {
			apply_outcome(
				*otherwise,
				working,
				process,
				pre_spawn_handler,
				post_spawn_handler,
				errors,
				events,
			)
			.await?;
		}

		(_, Outcome::Both(one, two)) => {
			if let Err(err) = apply_outcome(
				*one,
				working.clone(),
				process,
				pre_spawn_handler,
				post_spawn_handler,
				errors.clone(),
				events.clone(),
			)
			.await
			{
				debug!(
					"first outcome failed, sending an error but proceeding to the second anyway"
				);
				errors.send(err).await.ok();
			}

			apply_outcome(
				*two,
				working,
				process,
				pre_spawn_handler,
				post_spawn_handler,
				errors,
				events,
			)
			.await?;
		}
	}

	Ok(())
}
