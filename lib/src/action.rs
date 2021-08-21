//! Processor responsible for receiving events, filtering them, and scheduling actions in response.

use std::{
	fmt,
	sync::Arc,
	time::{Duration, Instant},
};

use atomic_take::AtomicTake;
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

#[derive(Clone, Debug)]
pub struct Action {
	pub events: Vec<Event>,
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

		let action = Action {
			events: set.drain(..).collect(),
		};
		debug!(?action, "action constructed");

		if let Some(h) = working.borrow().action_handler.take() {
			trace!("action handler updated");
			handler = h;
		}

		let err = handler.handle(action).map_err(|e| rte("action worker", e));
		if let Err(err) = err {
			errors.send(err).await?;
		}
	}
	Ok(())
}
