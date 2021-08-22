//! Processor responsible for receiving events, filtering them, and scheduling actions in response.

use std::{
	fmt,
	sync::Arc,
	time::{Duration, Instant},
};

use atomic_take::AtomicTake;
use command_group::Signal;
use once_cell::sync::OnceCell;
use tokio::{
	sync::{mpsc, watch},
	time::timeout,
};
use tracing::{debug, trace};

use crate::{
	command::Shell,
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
}

impl fmt::Debug for WorkingData {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("WorkingData")
			.field("throttle", &self.throttle)
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

	/// If the command isn't running, start it.
	Start,

	/// Wait for command completion, then start a new one.
	Queue,

	/// Stop the command, then start a new one.
	Restart,

	/// Send this signal to the command.
	Signal(Signal),

	/// When command is running, do the first, otherwise the second.
	IfRunning(Box<Outcome>, Box<Outcome>),

	/// Clear the screen before doing the inner outcome.
	ClearAnd(Box<Outcome>),
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

	pub fn clear_and(then: Outcome) -> Self {
		Self::ClearAnd(Box::new(then))
	}
}

pub async fn worker(
	working: watch::Receiver<WorkingData>,
	errors: mpsc::Sender<RuntimeError>,
	mut events: mpsc::Receiver<Event>,
) -> Result<(), CriticalError> {
	let mut last = Instant::now();
	let mut set = Vec::new();
	let mut handler =
		{ working.borrow().action_handler.take() }.ok_or(CriticalError::MissingHandler)?;

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
			handler = h;
		}

		let outcome = action.outcome.clone();
		let err = handler.handle(action).map_err(|e| rte("action worker", e));
		if let Err(err) = err {
			errors.send(err).await?;
		}

		let outcome = outcome.get().cloned().unwrap_or_default();
		debug!(?outcome, "handler finished");
	}

	debug!("action worker finished");
	Ok(())
}
