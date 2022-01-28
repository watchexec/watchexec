//! Processor responsible for receiving events, filtering them, and scheduling actions in response.

use std::{
	sync::Arc,
	time::{Duration, Instant},
};

use clearscreen::ClearScreen;
use tokio::{
	spawn,
	sync::{
		mpsc,
		watch::{self, Receiver},
		RwLock,
	},
	time::timeout,
};
use tracing::{debug, error, trace, warn};

use crate::{
	command::Supervisor,
	error::{CriticalError, RuntimeError},
	event::Event,
	handler::rte,
	signal::process::SubSignal,
};

#[doc(inline)]
pub use outcome::Outcome;
#[doc(inline)]
pub use workingdata::*;

mod outcome;
mod workingdata;

/// The main worker of a Watchexec process.
///
/// This is the main loop of the process. It receives events from the event channel, filters them,
/// debounces them, obtains the desired outcome of an actioned event, calls the appropriate handlers
/// and schedules processes as needed.
pub async fn worker(
	working: watch::Receiver<WorkingData>,
	errors: mpsc::Sender<RuntimeError>,
	events_tx: mpsc::Sender<Event>,
	mut events: mpsc::Receiver<Event>,
) -> Result<(), CriticalError> {
	let mut last = Instant::now();
	let mut set = Vec::new();
	let process = ProcessHolder::default();

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
				Ok(None) => break,
				Ok(Some(event)) => {
					trace!(?event, "got event");

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

		ActionOutcome {
			events,
			working: working.clone(),
			process: process.clone(),
			errors_c: errors.clone(),
			events_c: events_tx.clone(),
		}
		.spawn(outcome);
	}

	debug!("action worker finished");
	Ok(())
}

#[derive(Clone)]
struct ActionOutcome {
	events: Arc<[Event]>,
	working: Receiver<WorkingData>,
	process: ProcessHolder,
	errors_c: mpsc::Sender<RuntimeError>,
	events_c: mpsc::Sender<Event>,
}

impl ActionOutcome {
	fn spawn(self, outcome: Outcome) {
		debug!(?outcome, "spawning outcome applier");
		let this = self;
		spawn(async move {
			let errors_c = this.errors_c.clone();
			if let Err(err) = this.apply_outcome(outcome.clone()).await {
				error!(?err, "outcome applier errored");
				if let Err(err) = errors_c.send(err).await {
					error!(?err, "failed to send an error, something is terribly wrong");
				}
			} else {
				debug!(?outcome, "outcome applier finished");
			}
		});
	}

	#[async_recursion::async_recursion]
	async fn apply_outcome(&self, outcome: Outcome) -> Result<(), RuntimeError> {
		match (self.process.is_some().await, outcome) {
			(_, Outcome::DoNothing) => {}
			(_, Outcome::Exit) => {
				return Err(RuntimeError::Exit);
			}
			(true, Outcome::Stop) => {
				self.process.kill().await;
				self.process.wait().await?;
				self.process.drop_inner().await;
			}
			(false, o @ Outcome::Stop)
			| (false, o @ Outcome::Wait)
			| (false, o @ Outcome::Signal(_)) => {
				debug!(outcome=?o, "meaningless without a process, not doing anything");
			}
			(_, Outcome::Start) => {
				let (cmd, shell, grouped, pre_spawn_handler, post_spawn_handler) = {
					let wrk = self.working.borrow();
					(
						wrk.command.clone(),
						wrk.shell.clone(),
						wrk.grouped,
						wrk.pre_spawn_handler.clone(),
						wrk.post_spawn_handler.clone(),
					)
				};

				if cmd.is_empty() {
					warn!("tried to start a command without anything to run");
				} else {
					let command = shell.to_command(&cmd);
					let (pre_spawn, command) =
						PreSpawn::new(command, cmd.clone(), self.events.clone());

					debug!("running pre-spawn handler");
					pre_spawn_handler
						.call(pre_spawn)
						.await
						.map_err(|e| rte("action pre-spawn", e))?;

					let mut command = Arc::try_unwrap(command)
						.map_err(|_| RuntimeError::HandlerLockHeld("pre-spawn"))?
						.into_inner();

					trace!("spawning supervisor for command");
					let sup = Supervisor::spawn(
						self.errors_c.clone(),
						self.events_c.clone(),
						&mut command,
						grouped,
					)?;

					debug!("running post-spawn handler");
					let post_spawn = PostSpawn {
						command: cmd.clone(),
						events: self.events.clone(),
						id: sup.id(),
						grouped,
					};
					post_spawn_handler
						.call(post_spawn)
						.await
						.map_err(|e| rte("action post-spawn", e))?;

					self.process.replace(sup).await;
				}
			}

			(true, Outcome::Signal(sig)) => {
				self.process.signal(sig).await;
			}

			(true, Outcome::Wait) => {
				self.process.wait().await?;
			}

			(_, Outcome::Clear) => {
				clearscreen::clear()?;
			}

			(_, Outcome::Reset) => {
				for cs in [
					ClearScreen::WindowsCooked,
					ClearScreen::WindowsVt,
					ClearScreen::VtLeaveAlt,
					ClearScreen::VtWellDone,
					ClearScreen::default(),
				] {
					cs.clear()?;
				}
			}

			(true, Outcome::IfRunning(then, _)) => {
				self.apply_outcome(*then).await?;
			}
			(false, Outcome::IfRunning(_, otherwise)) => {
				self.apply_outcome(*otherwise).await?;
			}

			(_, Outcome::Both(one, two)) => {
				if let Err(err) = self.apply_outcome(*one).await {
					debug!(
						"first outcome failed, sending an error but proceeding to the second anyway"
					);
					self.errors_c.send(err).await.ok();
				}

				self.apply_outcome(*two).await?;
			}
		}

		Ok(())
	}
}

#[derive(Clone, Debug, Default)]
struct ProcessHolder(Arc<RwLock<Option<Supervisor>>>);
impl ProcessHolder {
	async fn is_running(&self) -> bool {
		self.0
			.read()
			.await
			.as_ref()
			.map(|p| p.is_running())
			.unwrap_or(false)
	}

	async fn is_some(&self) -> bool {
		self.0.read().await.is_some()
	}

	async fn drop_inner(&self) {
		self.0.write().await.take();
	}

	async fn replace(&self, new: Supervisor) {
		if let Some(_old) = self.0.write().await.replace(new) {
			// TODO: figure out what to do with old
		}
	}

	async fn signal(&self, sig: SubSignal) {
		if let Some(p) = self.0.read().await.as_ref() {
			p.signal(sig).await;
		}
	}

	async fn kill(&self) {
		if let Some(p) = self.0.read().await.as_ref() {
			p.kill().await;
		}
	}

	async fn wait(&self) -> Result<(), RuntimeError> {
		// Maybe loop this with a timeout to allow concurrent drop_inner?
		if let Some(p) = self.0.write().await.as_mut() {
			p.wait().await?; // TODO: &melf
		}

		Ok(())
	}
}
