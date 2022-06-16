use std::sync::{
	atomic::{AtomicUsize, Ordering},
	Arc,
};

use async_priority_channel as priority;
use clearscreen::ClearScreen;
use futures::Future;
use tokio::{
	spawn,
	sync::{mpsc, watch::Receiver},
	time::sleep,
};
use tracing::{debug, error, trace, warn};

use crate::{
	command::Supervisor,
	error::RuntimeError,
	event::{Event, Priority},
};

use super::{process_holder::ProcessHolder, Outcome, WorkingData};

#[derive(Clone)]
pub struct OutcomeWorker {
	events: Arc<[Event]>,
	working: Receiver<WorkingData>,
	process: ProcessHolder,
	gen: usize,
	gencheck: Arc<AtomicUsize>,
	errors_c: mpsc::Sender<RuntimeError>,
	events_c: priority::Sender<Event, Priority>,
}

impl OutcomeWorker {
	pub fn newgen() -> Arc<AtomicUsize> {
		Default::default()
	}

	pub fn spawn(
		outcome: Outcome,
		events: Arc<[Event]>,
		working: Receiver<WorkingData>,
		process: ProcessHolder,
		gencheck: Arc<AtomicUsize>,
		errors_c: mpsc::Sender<RuntimeError>,
		events_c: priority::Sender<Event, Priority>,
	) {
		let gen = gencheck.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
		let this = Self {
			events,
			working,
			process,
			gen,
			gencheck,
			errors_c,
			events_c,
		};

		debug!(?outcome, %gen, "spawning outcome worker");
		spawn(async move {
			let errors_c = this.errors_c.clone();
			match this.apply(outcome.clone()).await {
				Err(err) => {
					if matches!(err, RuntimeError::Exit) {
						debug!(%gen, "propagating graceful exit");
					} else {
						error!(?err, %gen, "outcome applier errored");
					}

					if let Err(err) = errors_c.send(err).await {
						error!(?err, %gen, "failed to send an error, something is terribly wrong");
					}
				}
				Ok(_) => {
					debug!(?outcome, %gen, "outcome worker finished");
				}
			}
		});
	}

	async fn check_gen<O>(&self, f: impl Future<Output = O>) -> Option<O> {
		// TODO: use a select and a notifier of some kind so it cancels tasks
		if self.gencheck.load(Ordering::SeqCst) != self.gen {
			warn!(when=%"pre", gen=%self.gen, "outcome worker was cycled, aborting");
			return None;
		}
		let o = f.await;
		if self.gencheck.load(Ordering::SeqCst) != self.gen {
			warn!(when=%"post", gen=%self.gen, "outcome worker was cycled, aborting");
			return None;
		}
		Some(o)
	}

	#[async_recursion::async_recursion]
	async fn apply(&self, outcome: Outcome) -> Result<(), RuntimeError> {
		macro_rules! notry {
			($e:expr) => {
				match self.check_gen($e).await {
					None => return Ok(()),
					Some(o) => o,
				}
			};
		}
		match (notry!(self.process.is_some()), outcome) {
			(_, Outcome::DoNothing) => {}
			(_, Outcome::Exit) => {
				return Err(RuntimeError::Exit);
			}
			(true, Outcome::Stop) => {
				notry!(self.process.kill());
				notry!(self.process.wait())?;
				notry!(self.process.drop_inner());
			}
			(false, o @ Outcome::Stop)
			| (false, o @ Outcome::Wait)
			| (false, o @ Outcome::Signal(_)) => {
				debug!(outcome=?o, "meaningless without a process, not doing anything");
			}
			(_, Outcome::Start) => {
				let (cmds, grouped, pre_spawn_handler, post_spawn_handler) = {
					let wrk = self.working.borrow();
					(
						wrk.commands.clone(),
						wrk.grouped,
						wrk.pre_spawn_handler.clone(),
						wrk.post_spawn_handler.clone(),
					)
				};

				if cmds.is_empty() {
					warn!("tried to start commands without anything to run");
				} else {
					trace!("spawning supervisor for command");
					let sup = Supervisor::spawn(
						self.errors_c.clone(),
						self.events_c.clone(),
						cmds,
						grouped,
						self.events.clone(),
						pre_spawn_handler,
						post_spawn_handler,
					)?;
					notry!(self.process.replace(sup));
				}
			}

			(true, Outcome::Signal(sig)) => {
				notry!(self.process.signal(sig));
			}

			(true, Outcome::Wait) => {
				notry!(self.process.wait())?;
			}

			(_, Outcome::Sleep(time)) => {
				trace!(?time, "sleeping");
				notry!(sleep(time));
				trace!(?time, "done sleeping");
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
				notry!(self.apply(*then))?;
			}
			(false, Outcome::IfRunning(_, otherwise)) => {
				notry!(self.apply(*otherwise))?;
			}

			(_, Outcome::Both(one, two)) => {
				if let Err(err) = notry!(self.apply(*one)) {
					debug!(
						"first outcome failed, sending an error but proceeding to the second anyway"
					);
					notry!(self.errors_c.send(err)).ok();
				}

				notry!(self.apply(*two))?;
			}
		}

		Ok(())
	}
}
