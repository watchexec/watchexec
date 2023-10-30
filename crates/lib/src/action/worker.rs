use std::{
	collections::HashMap,
	mem::take,
	sync::{atomic::AtomicUsize, Arc},
	time::{Duration, Instant},
};

use async_priority_channel as priority;
use tokio::{sync::mpsc, time::timeout};
use tracing::{debug, trace};
use watchexec_events::{Event, Priority};

use crate::{
	action::{
		outcome_worker::OutcomeWorker, process_holder::ProcessHolder, Action, EventSet,
		InstanceOrder, Outcome, SupervisionOrder,
	},
	command::{Command, SupervisorId},
	error::{CriticalError, RuntimeError},
	filter::Filterer,
	Config,
};

/// The main worker of a Watchexec process.
///
/// This is the main loop of the process. It receives events from the event channel, filters them,
/// debounces them, obtains the desired outcome of an actioned event, calls the appropriate handlers
/// and schedules processes as needed.
pub async fn worker(
	config: Arc<Config>,
	errors: mpsc::Sender<RuntimeError>,
	events_tx: priority::Sender<Event, Priority>,
	events: priority::Receiver<Event, Priority>,
) -> Result<(), CriticalError> {
	let mut processes: HashMap<SupervisorId, ProcessData> = HashMap::new();

	while let Some(mut set) = throttle_collect(
		config.clone(),
		events.clone(),
		errors.clone(),
		Instant::now(),
	)
	.await?
	{
		let events = Arc::from(take(&mut set).into_boxed_slice());

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

		// grab order arcs before running handler
		let supervision_orders = action.supervision.clone();
		let instance_orders = action.instance.clone();

		debug!("running action handler");
		config.action_handler.call(action);

		debug!("apply orders to instance");
		let instance_orders = take(&mut *instance_orders.lock().expect("lock poisoned"));
		#[allow(clippy::never_loop)] // doesn't yet, but will in future
		for order in instance_orders {
			match order {
				InstanceOrder::Quit => {
					// TODO: make this a signal or something so we can uphold the promise that
					//       calling apply() before quit() lets you gracefully stop processes.
					break; // end the worker, which will quit watchexec
				}
			}
		}

		debug!("apply orders to supervisors");
		let supervision_orders = take(&mut *supervision_orders.lock().expect("lock poisoned"));
		for (id, orders) in supervision_orders {
			// the action handler should produce at most a single Apply out of the box
			debug_assert!(
				orders
					.iter()
					.filter(|o| matches!(o, SupervisionOrder::Apply(_, _)))
					.count() <= 1
			);

			// TODO process each process in parallel, but each order in series
			for order in orders {
				let (found, event_set) = match order {
					SupervisionOrder::Clear => todo!(),
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
								config: config.clone(),
								command: command.clone(),
								process: ProcessHolder::default(),
								outcome_gen: OutcomeWorker::newgen(),
							},
						);
						(None, None)
					}
					SupervisionOrder::Destroy => {
						debug!(?id, "destroying supervisor");
						(
							processes.remove(&id).map(|data| (Outcome::Destroy, data)),
							None,
						)
					}
				};

				// FIXME: need to collect entire Outcome from all orders for a process

				let Some((
					outcome,
					ProcessData {
						config,
						command,
						process,
						outcome_gen,
					},
				)) = found
				else {
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
					config.clone(),
					outcome,
					events.clone(),
					command,
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
	config: Arc<Config>,
	command: Command,
	process: ProcessHolder,
	outcome_gen: Arc<AtomicUsize>,
}

pub async fn throttle_collect(
	config: Arc<Config>,
	events: priority::Receiver<Event, Priority>,
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
			config.throttle.get().saturating_sub(last.elapsed())
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
						let filtered = config.filterer.check_event(&event, priority);
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
						if elapsed < config.throttle.get() {
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
