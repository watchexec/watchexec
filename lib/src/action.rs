//! Processor responsible for receiving events, filtering them, and scheduling actions in response.

use std::{
	fmt,
	sync::Arc,
	time::{Duration, Instant},
};

use atomic_take::AtomicTake;
use command_group::{AsyncCommandGroup, Signal};
use once_cell::sync::OnceCell;
use tokio::{
	sync::{mpsc, watch},
	time::timeout,
};
use tracing::{debug, trace, warn};

use crate::{
	command::{Process, Shell},
	error::{CriticalError, RuntimeError},
	event::Event,
	handler::{rte, Handler},
};

#[derive(Clone)]
#[non_exhaustive]
pub struct WorkingData {
	pub throttle: Duration,

	pub action_handler: Arc<AtomicTake<Box<dyn Handler<Action> + Send>>>,

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
	pub fn outcome(self, outcome: Outcome) {
		self.outcome.set(outcome).ok();
	}
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

	// TODO
	// /// Wait for command completion, then start a new one.
	// Queue,
	/// Send this signal to the command.
	Signal(Signal),

	/// Clear the screen.
	Clear,

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
	mut events: mpsc::Receiver<Event>,
) -> Result<(), CriticalError> {
	let mut last = Instant::now();
	let mut set = Vec::new();
	let mut action_handler =
		{ working.borrow().action_handler.take() }.ok_or(CriticalError::MissingHandler)?;
	let mut process: Option<Process> = None;

	loop {
		let maxtime = working.borrow().throttle.saturating_sub(last.elapsed());

		if maxtime.is_zero() {
			trace!("out of throttle on recycle");
		} else {
			trace!(?maxtime, "waiting for event");
			match timeout(maxtime, events.recv()).await {
				Err(_timeout) => {
					trace!("timed out");
				}
				Ok(None) => break,
				Ok(Some(event)) => {
					trace!(?event, "got event");
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

		let outcome = action.outcome.clone();
		let err = action_handler.handle(action).map_err(|e| rte("action worker", e));
		if let Err(err) = err {
			errors.send(err).await?;
		}

		let outcome = outcome.get().cloned().unwrap_or_default();
		debug!(?outcome, "handler finished");

		let is_running = match process.as_mut().map(|p| p.is_running()).transpose() {
			Err(err) => {
				errors.send(err).await?;
				false
			}
			Ok(Some(ir)) => ir,
			Ok(None) => false,
		};

		let outcome = outcome.resolve(is_running);
		debug!(?outcome, "outcome resolved");

		let w = working.borrow().clone();
		let rerr = apply_outcome(outcome, w, &mut process).await;
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
	process: &mut Option<Process>,
) -> Result<(), RuntimeError> {
	match (process.as_mut(), outcome) {
		(_, Outcome::DoNothing) => {}
		(_, Outcome::Exit) => {
			return Err(RuntimeError::Exit);
		}
		(Some(p), Outcome::Stop) => {
			p.kill().await?;
			p.wait().await?;
		}
		(p @ None, o @ Outcome::Stop)
		| (p @ Some(_), o @ Outcome::Start)
		| (p @ None, o @ Outcome::Signal(_)) => {
			warn!(is_running=?p.is_some(), outcome=?o, "outcome does not apply to process state");
		}
		(None, Outcome::Start) => {
			if working.command.is_empty() {
				warn!("tried to start a command without anything to run");
			} else {
			let mut command = working.shell.to_command(&working.command);

			// TODO: pre-spawn hook

			let proc = if working.grouped {
				Process::Grouped(command.group_spawn()?)
			} else {
				Process::Ungrouped(command.spawn()?)
			};

			// TODO: post-spawn hook

			*process = Some(proc);
			}
		}

		(Some(p), Outcome::Signal(sig)) => {
			// TODO: windows
			p.signal(sig)?;
		}

		(_, Outcome::Clear) => {
			clearscreen::clear()?;
		}

		(Some(_), Outcome::IfRunning(then, _)) => {
			apply_outcome(*then, working, process).await?;
		}
		(None, Outcome::IfRunning(_, otherwise)) => {
			apply_outcome(*otherwise, working, process).await?;
		}

		(_, Outcome::Both(one, two)) => {
			apply_outcome(*one, working.clone(), process).await?;
			apply_outcome(*two, working, process).await?;
		}
	}

	Ok(())
}
