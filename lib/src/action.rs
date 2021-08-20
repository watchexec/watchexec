//! Processor responsible for receiving events, filtering them, and scheduling actions in response.

use std::time::Duration;

use tokio::sync::{mpsc, watch};

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
	mut working: watch::Receiver<WorkingData>,
	errors: mpsc::Sender<RuntimeError>,
	events: mpsc::Receiver<Event>,
) -> Result<(), CriticalError> {
	Ok(())
}
