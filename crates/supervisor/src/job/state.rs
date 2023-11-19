use std::time::Instant;

#[cfg(not(test))]
use command_group::{tokio::ErasedChild, AsyncCommandGroup};
use tokio::process::Command as TokioCommand;
use watchexec_events::ProcessEnd;

use crate::command::Command;

#[derive(Debug)]
#[cfg_attr(test, derive(Clone))]
pub enum CommandState {
	ToRun,
	IsRunning {
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
	#[cfg_attr(test, allow(unused_mut, unused_variables))]
	pub(crate) async fn spawn(
		&mut self,
		command: Command,
		mut spawnable: TokioCommand,
	) -> std::io::Result<bool> {
		if let Self::IsRunning { .. } = self {
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

		*self = Self::IsRunning {
			child,
			started: Instant::now(),
		};
		Ok(true)
	}

	#[must_use]
	pub(crate) fn reset(&mut self) -> Self {
		match self {
			Self::ToRun => Self::ToRun,
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

				*self = Self::ToRun;
				copy
			}
			Self::IsRunning { started, .. } => {
				let copy = Self::Finished {
					status: ProcessEnd::Continued,
					started: *started,
					finished: Instant::now(),
				};

				*self = Self::ToRun;
				copy
			}
		}
	}
}
