use std::time::Instant;

#[cfg(not(test))]
use command_group::{tokio::ErasedChild, AsyncCommandGroup};
use tokio::process::Command as TokioCommand;
use watchexec_events::ProcessEnd;

use crate::command::Command;

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
pub enum CommandState {
	Pending,
	Running {
		#[cfg(test)]
		child: super::TestChild,
		#[cfg(not(test))]
		child: ErasedChild,
		started: Instant,
	},
	Finished {
		status: ProcessEnd,
		started: Instant,
		finished: Instant,
	},
}

impl CommandState {
	/// Whether the command is pending, i.e. not running or finished.
	pub fn is_pending(&self) -> bool {
		matches!(self, Self::Pending)
	}

	/// Whether the command is running.
	pub fn is_running(&self) -> bool {
		matches!(self, Self::Running { .. })
	}

	/// Whether the command is finished.
	pub fn is_finished(&self) -> bool {
		matches!(self, Self::Finished { .. })
	}

	#[cfg_attr(test, allow(unused_mut, unused_variables))]
	pub(crate) async fn spawn(
		&mut self,
		command: Command,
		mut spawnable: TokioCommand,
	) -> std::io::Result<bool> {
		if let Self::Running { .. } = self {
			return Ok(false);
		}

		#[cfg(test)]
		let child = super::TestChild::new(command)?;

		#[cfg(not(test))]
		let child = if command.grouped {
			ErasedChild::Grouped(spawnable.group().spawn()?)
		} else {
			ErasedChild::Ungrouped(spawnable.spawn()?)
		};

		*self = Self::Running {
			child,
			started: Instant::now(),
		};
		Ok(true)
	}

	#[must_use]
	pub(crate) fn reset(&mut self) -> Self {
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
			let end = child.wait().await?;
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
