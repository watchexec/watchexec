use std::{sync::Arc, time::Instant};

use process_wrap::tokio::{TokioChildWrapper, TokioCommandWrap};
use tracing::trace;
use watchexec_events::ProcessEnd;

use crate::command::Command;

/// The state of the job's command / process.
///
/// This is used both internally to represent the current state (ready/pending, running, finished)
/// of the command, and can be queried via the [`JobTaskContext`](super::JobTaskContext) by hooks.
///
/// Technically, some operations can be done through a `&self` shared borrow on the running
/// command's [`TokioChildWrapper`], but this library recommends against taking advantage of this,
/// and prefer using the methods on [`Job`](super::Job) instead, so that the job can keep track of
/// what's going on.
#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
pub enum CommandState {
	/// The command is neither running nor has finished. This is the initial state.
	Pending,

	/// The command is currently running. Note that this is established after the process is spawned
	/// and not precisely synchronised with the process' aliveness: in some cases the process may be
	/// exited but still `Running` in this enum.
	Running {
		/// The child process (test version).
		#[cfg(test)]
		child: super::TestChild,

		/// The child process.
		#[cfg(not(test))]
		child: Box<dyn TokioChildWrapper>,

		/// The time at which the process was spawned.
		started: Instant,
	},

	/// The command has completed and its status was collected.
	Finished {
		/// The command's exit status.
		status: ProcessEnd,

		/// The time at which the process was spawned.
		started: Instant,

		/// The time at which the process finished, or more precisely, when its status was collected.
		finished: Instant,
	},
}

impl CommandState {
	/// Whether the command is pending, i.e. not running or finished.
	#[must_use]
	pub const fn is_pending(&self) -> bool {
		matches!(self, Self::Pending)
	}

	/// Whether the command is running.
	#[must_use]
	pub const fn is_running(&self) -> bool {
		matches!(self, Self::Running { .. })
	}

	/// Whether the command is finished.
	#[must_use]
	pub const fn is_finished(&self) -> bool {
		matches!(self, Self::Finished { .. })
	}

	#[cfg_attr(test, allow(unused_mut, unused_variables))]
	pub(crate) fn spawn(
		&mut self,
		command: Arc<Command>,
		mut spawnable: TokioCommandWrap,
	) -> std::io::Result<bool> {
		if let Self::Running { .. } = self {
			trace!("command running, not spawning again");
			return Ok(false);
		}

		trace!(?command, "spawning command");

		#[cfg(test)]
		let child = super::TestChild::new(command)?;

		#[cfg(not(test))]
		let child = spawnable.spawn()?;

		*self = Self::Running {
			child,
			started: Instant::now(),
		};
		Ok(true)
	}

	#[must_use]
	pub(crate) fn reset(&mut self) -> Self {
		trace!(?self, "resetting command state");
		match self {
			Self::Pending => Self::Pending,
			Self::Finished {
				status,
				started,
				finished,
				..
			} => {
				let copy = Self::Finished {
					status: *status,
					started: *started,
					finished: *finished,
				};

				*self = Self::Pending;
				copy
			}
			Self::Running { started, .. } => {
				let copy = Self::Finished {
					status: ProcessEnd::Continued,
					started: *started,
					finished: Instant::now(),
				};

				*self = Self::Pending;
				copy
			}
		}
	}

	pub(crate) async fn wait(&mut self) -> std::io::Result<bool> {
		if let Self::Running { child, started } = self {
			let end = Box::into_pin(child.wait()).await?;
			*self = Self::Finished {
				status: end.into(),
				started: *started,
				finished: Instant::now(),
			};
			Ok(true)
		} else {
			Ok(false)
		}
	}
}
