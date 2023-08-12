use std::{
	collections::HashMap,
	mem::take,
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
	action::{
		outcome_worker::OutcomeWorker, process_holder::ProcessHolder, Action, EventSet, Outcome,
		SupervisionOrder, WorkingData,
	},
	command::{Command, SupervisorId},
	error::{CriticalError, RuntimeError},
	event::{Event, Priority},
	handler::rte,
};

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

		trace!("preparing action handler");
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
		let action_handler = {
			let wrk = working.borrow();
			wrk.action_handler.clone()
		};

		// grab order arcs before running handler
		let supervision_orders = action.supervision.clone();
		let _instance_orders = action.instance.clone(); // TODO

		debug!("running action handler");
		let err = action_handler
			.call(action)
			.await
			.map_err(|e| rte("action worker", e.as_ref()));
		if let Err(err) = err {
			errors.send(err).await?;
			debug!("action handler errored, skipping");
			continue;
		}

		// FIXME: process instance orders

		debug!("apply orders to supervisors");
		let supervision_orders = take(&mut *supervision_orders.lock().expect("lock poisoned"));
		for (id, orders) in supervision_orders {
			// TODO process each process in parallel, but each order in series
			for order in orders {
				let (found, event_set) = match order {
					SupervisionOrder::Apply(outcome, event_set) => {
						debug!(?id, ?outcome, "apply outcome to supervisor");
						(
							processes
								.get(&id)
								.map(|data| (outcome.clone(), data.clone())),
							Some(event_set),
						)
					}
					SupervisionOrder::Create(command) => {
						debug_assert!(!processes.contains_key(&id));
						debug!(?id, ?command, "creating new supervisor");
						processes.insert(
							id,
							ProcessData {
								// due to borrow semantics, workingdata lock is only held for this line
								working: working.clone(),
								command: command.clone(),
								process: ProcessHolder::default(),
								outcome_gen: OutcomeWorker::newgen(),
							},
						);
						(None, None)
					}
					SupervisionOrder::Remove => {
						debug!(?id, "removing supervisor");
						(
							processes.remove(&id).map(|data| {
								(Outcome::if_running(Outcome::Stop, Outcome::DoNothing), data)
							}),
							None,
						)
					}
				};

				// FIXME: need to collect entire Outcome from all orders for a process

				let Some((outcome, ProcessData { working, command, process, outcome_gen })) = found else {
				continue;
			};

				let outcome = outcome.resolve(process.is_running().await);
				debug!(?outcome, "outcome resolved");

				let events = match event_set {
					Some(EventSet::All) => events.clone(),
					None | Some(EventSet::None) => Arc::from(Vec::new().into_boxed_slice()),
					Some(EventSet::Some(selected)) => {
						Arc::from(selected.clone().into_boxed_slice())
					}
				};
				debug!(?events, "events selected");
				OutcomeWorker::spawn(
					outcome,
					events.clone(),
					command,
					working.clone(),
					process.clone(),
					id,
					outcome_gen.clone(),
					errors.clone(),
					events_tx.clone(),
				);
				debug!("action process done");
			}
		}

		debug!("action handler finished");
	}

	debug!("action worker finished");
	Ok(())
}

// FIXME: dedicated file/struct for supervisorset
#[derive(Clone, Debug)]
struct ProcessData {
	working: watch::Receiver<WorkingData>,
	command: Command,
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
