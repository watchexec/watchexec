use std::{
	collections::HashMap,
	sync::{atomic::AtomicUsize, Arc},
	time::{Duration, Instant},
};

use async_priority_channel as priority;
use tokio::{
	sync::{mpsc, watch},
	time::timeout,
};
use tracing::{debug, trace};

use crate::{
	action::{EventSet, Outcome, Resolution},
	command::{Command, SupervisorId},
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
	let mut processes: HashMap<SupervisorId, ProcessData> = HashMap::new();

	while let Some(mut set) = throttle_collect(
		events.clone(),
		working.clone(),
		errors.clone(),
		Instant::now(),
	)
	.await?
	{
		#[allow(clippy::iter_with_drain)]
		let events = Arc::from(set.drain(..).collect::<Vec<_>>().into_boxed_slice());
		let action = Action::new(
			Arc::clone(&events),
			Arc::from(
				processes
					.keys()
					.copied()
					.collect::<Vec<_>>()
					.into_boxed_slice(),
			),
		);
		debug!("running action handler");
		let action_handler = {
			let wrk = working.borrow();
			wrk.action_handler.clone()
		};

		let outcomes = action.outcomes.clone();
		let err = action_handler
			.call(action)
			.await
			.map_err(|e| rte("action worker", e.as_ref()));
		if let Err(err) = err {
			errors.send(err).await?;
			debug!("action handler errored, skipping");
			continue;
		}

		// TODO(Felix) would you prefer this to be handled differently to avoid the potential
		// misbehavior suggested in this lint?
		#[allow(clippy::nursery)]
		for (pid, (resolution, event_set)) in outcomes.lock().await.iter() {
			let found = match resolution {
				Resolution::Apply(outcome) => {
					debug!(pid=?pid, outocome=?outcome, "apply outcome to alive command");
					processes
						.get(pid)
						.map(|data| (outcome.clone(), data.clone()))
				}
				Resolution::Start(cmds) => {
					assert!(processes.get(pid).is_none());
					debug!(pid=?pid, cmds=?cmds, "starting new command");
					// due to borrow semantics, lock is only held for this line
					let data = ProcessData {
						working: working.clone(),
						commands: cmds.clone(),
						process: ProcessHolder::default(),
						outcome_gen: OutcomeWorker::newgen(),
					};

					processes.insert(*pid, data.clone());

					Some((Outcome::Start, data))
				}
				Resolution::Remove => {
					// command only stopped if it exists
					debug!(pid=?pid, "removing command");
					processes.remove(pid).map(|data| (Outcome::Stop, data))
				}
			};

			let Some((outcome, ProcessData { working, commands, process, outcome_gen })) = found else {
				continue;
			};

			let outcome = outcome.resolve(process.is_running().await);
			debug!(?outcome, "outcome resolved");

			let events = match event_set {
				EventSet::All => events.clone(),
				EventSet::None => Arc::from(vec![].into_boxed_slice()),
				EventSet::Some(selected) => Arc::from(selected.clone().into_boxed_slice()),
			};
			debug!(?events, "events selected");
			OutcomeWorker::spawn(
				outcome,
				events.clone(),
				commands,
				working.clone(),
				process.clone(),
				*pid,
				outcome_gen.clone(),
				errors.clone(),
				events_tx.clone(),
			);
			debug!("action process done");
		}

		debug!("action handler finished");
	}

	debug!("action worker finished");
	Ok(())
}

#[derive(Clone)]
struct ProcessData {
	working: watch::Receiver<WorkingData>,
	commands: Vec<Command>,
	process: ProcessHolder,
	outcome_gen: Arc<AtomicUsize>,
}

pub async fn throttle_collect(
	events: priority::Receiver<Event, Priority>,
	working: watch::Receiver<WorkingData>,
	errors: mpsc::Sender<RuntimeError>,
	mut last: Instant,
) -> Result<Option<Vec<Event>>, CriticalError> {
	if events.is_closed() {
		trace!("events channel closed, stopping");
		return Ok(None);
	}
	let mut set: Vec<Event> = vec![];
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
			}

			trace!("out of throttle on recycle");
		} else {
			trace!(?maxtime, "waiting for event");
			let maybe_event = timeout(maxtime, events.recv()).await;
			if events.is_closed() {
				trace!("events channel closed during timeout, stopping");
				return Ok(None);
			}

			match maybe_event {
				Err(_timeout) => {
					trace!("timed out, cycling");
					continue;
				}
				Ok(Err(_empty)) => return Ok(None),
				Ok(Ok((event, priority))) => {
					trace!(?event, ?priority, "got event");

					if priority == Priority::Urgent {
						trace!("urgent event, by-passing filters");
					} else if event.is_empty() {
						trace!("empty event, by-passing filters");
					} else {
						let filtered = working.borrow().filterer.check_event(&event, priority);
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

					if priority == Priority::Urgent {
						trace!("urgent event, by-passing throttle");
					} else {
						let elapsed = last.elapsed();
						if elapsed < working.borrow().throttle {
							trace!(?elapsed, "still within throttle window, cycling");
							continue;
						}
					}
				}
			}
		}
		return Ok(Some(set));
	}
}
