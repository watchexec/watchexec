use std::{
	sync::Arc,
	time::{Duration, Instant},
};

use async_priority_channel as priority;
use tokio::{
	sync::{
		mpsc,
		watch::{self},
	},
	time::timeout,
};
use tracing::{debug, trace};

use crate::{
	error::{CriticalError, RuntimeError},
	event::{Event, Priority},
	handler::rte,
};

use super::{outcome_worker::OutcomeWorker, process_holder::ProcessHolder, Action, WorkingData};

/// The main worker of a Watchexec process.
///
/// This is the main loop of the process. It receives events from the event channel, filters them,
/// debounces them, obtains the desired outcome of an actioned event, calls the appropriate handlers
/// and schedules processes as needed.
pub async fn worker(
	working: watch::Receiver<WorkingData>,
	errors: mpsc::Sender<RuntimeError>,
	events_tx: priority::Sender<Event, Priority>,
	events: priority::Receiver<Event, Priority>,
) -> Result<(), CriticalError> {
	let mut last = Instant::now();
	let mut set = Vec::new();
	let process = ProcessHolder::default();
	let outcome_gen = OutcomeWorker::newgen();

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
				Ok(Err(_empty)) => break,
				Ok(Ok((event, priority))) => {
					trace!(?event, ?priority, "got event");

					if event.is_empty() {
						trace!("empty event, by-passing filters");
					} else {
						let filtered = working.borrow().filterer.check_event(&event);
						match filtered {
							Err(err) => {
								trace!(%err, "filter errored on event");
								errors.send(err).await?;
								continue;
							}
							Ok(false) => {
								trace!("filter rejected event");
								continue;
							}
							Ok(true) => {
								trace!("filter passed event");
							}
						}
					}

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

		let events = Arc::from(set.drain(..).collect::<Vec<_>>().into_boxed_slice());
		let action = Action::new(Arc::clone(&events));
		debug!(?action, "action constructed");

		debug!("running action handler");
		let action_handler = {
			let wrk = working.borrow();
			wrk.action_handler.clone()
		};

		let outcome = action.outcome.clone();
		let err = action_handler
			.call(action)
			.await
			.map_err(|e| rte("action worker", e));
		if let Err(err) = err {
			errors.send(err).await?;
			debug!("action handler errored, skipping");
			continue;
		}

		let outcome = outcome.get().cloned().unwrap_or_default();
		debug!(?outcome, "handler finished");

		let outcome = outcome.resolve(process.is_running().await);
		debug!(?outcome, "outcome resolved");

		OutcomeWorker::spawn(
			outcome,
			events,
			working.clone(),
			process.clone(),
			outcome_gen.clone(),
			errors.clone(),
			events_tx.clone(),
		);
		debug!("action process done");
	}

	debug!("action worker finished");
	Ok(())
}
