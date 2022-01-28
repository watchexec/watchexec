use std::sync::Arc;

use clearscreen::ClearScreen;
use tokio::{
	spawn,
	sync::{mpsc, watch::Receiver},
};
use tracing::{debug, error, trace, warn};

use crate::{command::Supervisor, error::RuntimeError, event::Event, handler::rte};

use super::{process_holder::ProcessHolder, Outcome, PostSpawn, PreSpawn, WorkingData};

#[derive(Clone)]
pub struct OutcomeWorker {
	events: Arc<[Event]>,
	working: Receiver<WorkingData>,
	process: ProcessHolder,
	errors_c: mpsc::Sender<RuntimeError>,
	events_c: mpsc::Sender<Event>,
}

impl OutcomeWorker {
	pub fn spawn(
		outcome: Outcome,
		events: Arc<[Event]>,
		working: Receiver<WorkingData>,
		process: ProcessHolder,
		errors_c: mpsc::Sender<RuntimeError>,
		events_c: mpsc::Sender<Event>,
	) {
		let this = Self {
			events,
			working,
			process,
			errors_c,
			events_c,
		};

		debug!(?outcome, "spawning outcome worker");
		spawn(async move {
			let errors_c = this.errors_c.clone();
			if let Err(err) = this.apply(outcome.clone()).await {
				if matches!(err, RuntimeError::Exit) {
					debug!("propagating graceful exit");
				} else {
					error!(?err, "outcome applier errored");
				}

				if let Err(err) = errors_c.send(err).await {
					error!(?err, "failed to send an error, something is terribly wrong");
				}
			} else {
				debug!(?outcome, "outcome worker finished");
			}
		});
	}

	#[async_recursion::async_recursion]
	async fn apply(&self, outcome: Outcome) -> Result<(), RuntimeError> {
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
				self.apply(*then).await?;
			}
			(false, Outcome::IfRunning(_, otherwise)) => {
				self.apply(*otherwise).await?;
			}

			(_, Outcome::Both(one, two)) => {
				if let Err(err) = self.apply(*one).await {
					debug!(
						"first outcome failed, sending an error but proceeding to the second anyway"
					);
					self.errors_c.send(err).await.ok();
				}

				self.apply(*two).await?;
			}
		}

		Ok(())
	}
}
