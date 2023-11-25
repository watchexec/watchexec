use std::{
	collections::HashMap,
	mem::take,
	sync::Arc,
	time::{Duration, Instant},
};

use async_priority_channel as priority;
use tokio::{sync::mpsc, time::timeout};
use tracing::{debug, trace};
use watchexec_events::{Event, Priority};
use watchexec_supervisor::job::Job;

use crate::{
	action::{Action, QuitManner},
	error::{CriticalError, RuntimeError},
	filter::Filterer,
	id::Id,
	late_join_set::LateJoinSet,
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
	events: priority::Receiver<Event, Priority>,
) -> Result<(), CriticalError> {
	let mut jobtasks = LateJoinSet::default();
	let mut jobs = HashMap::<Id, Job>::new();

	while let Some(mut set) = throttle_collect(
		config.clone(),
		events.clone(),
		errors.clone(),
		Instant::now(),
	)
	.await?
	{
		let events: Arc<[Event]> = Arc::from(take(&mut set).into_boxed_slice());

		trace!("preparing action handler");
		let action = Action::new(events.clone(), jobs.clone());

		debug!("running action handler");
		let action = config.action_handler.call(action);

		debug!("take control of new tasks");
		for (id, (job, task)) in action.new {
			trace!(?id, "taking control of new task");
			jobtasks.insert(task);
			jobs.insert(id, job);
		}

		if let Some(manner) = action.quit {
			debug!(?manner, "quitting worker");
			match manner {
				QuitManner::Abort => break,
				QuitManner::Graceful { signal, grace } => {
					debug!(?signal, ?grace, "quitting worker gracefully");
					let mut tasks = LateJoinSet::default();
					for (id, job) in jobs.drain() {
						trace!(?id, "quitting job");
						tasks.spawn(async move {
							job.stop_with_signal(signal, grace);
							job.delete().await;
						});
					}
					debug!("waiting for graceful shutdown tasks");
					tasks.join_all().await;
					debug!("waiting for job tasks to end");
					jobtasks.join_all().await;
					break;
				}
			}
		}

		debug!("action handler finished");
	}

	debug!("action worker finished");
	Ok(())
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
