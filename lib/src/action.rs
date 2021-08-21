//! Processor responsible for receiving events, filtering them, and scheduling actions in response.

use std::time::{Duration, Instant};

use tokio::{sync::{mpsc, watch}, time::timeout};

use crate::{
	error::{CriticalError, RuntimeError},
	event::Event,
};

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct WorkingData {
	pub throttle: Duration,
}

impl Default for WorkingData {
	fn default() -> Self {
		Self {
			throttle: Duration::from_millis(100),
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

	loop {
		let maxtime = working.borrow().throttle;
		match timeout(maxtime, events.recv()).await {
			Err(_timeout) => {},
			Ok(None) => break,
			Ok(Some(event)) => {
				set.push(event);

				if last.elapsed() < working.borrow().throttle {
					continue;
				}
			}
		}

		last = Instant::now();
		set.drain(..); // TODO: do action with the set
	}
	Ok(())
}
