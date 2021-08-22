//! Processor responsible for receiving events, filtering them, and scheduling actions in response.

use std::{
	fmt,
	sync::Arc,
	time::{Duration, Instant},
};

use atomic_take::AtomicTake;
use once_cell::sync::OnceCell;
use tokio::{
	sync::{mpsc, watch},
	time::timeout,
};
use tracing::{debug, trace};

use crate::{
	error::{CriticalError, RuntimeError},
	event::Event,
	handler::{rte, Handler},
};

#[derive(Clone)]
#[non_exhaustive]
pub struct WorkingData {
	pub throttle: Duration,
	pub action_handler: Arc<AtomicTake<Box<dyn Handler<Action> + Send>>>,
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
			throttle: Duration::from_millis(100),
			action_handler: Arc::new(AtomicTake::new(Box::new(()) as _)),
		}
	}
}

#[derive(Debug, Default)]
pub struct Action {
	pub events: Vec<Event>,
	outcome: Arc<OnceCell<Outcome>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
	DoNothing, // TODO more
}

impl Default for Outcome {
	fn default() -> Self {
		Self::DoNothing // TODO
	}
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
		let maxtime = working.borrow().throttle;
		match timeout(maxtime, events.recv()).await {
			Err(_timeout) => {}
			Ok(None) => break,
			Ok(Some(event)) => {
				set.push(event);

				if last.elapsed() < working.borrow().throttle {
					continue;
				}
			}
		}

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
	Ok(())
}
