use std::time::Instant;

#[cfg(not(test))]
use command_group::{tokio::ErasedChild, AsyncCommandGroup};
use tokio::process::Command as TokioCommand;
use watchexec_events::ProcessEnd;

use crate::command::Command;

#[derive(Debug)]
pub enum CommandState {
	ToRun(Command),
	IsRunning {
		command: Command,
		#[cfg(test)]
		child: super::TestChild,
		#[cfg(not(test))]
		child: ErasedChild,
		started: Instant,
	},
	Finished {
		command: Command,
		status: ProcessEnd,
		started: Instant,
		finished: Instant,
	},
}

impl CommandState {
	#[cfg_attr(test, allow(unused_mut))]
	pub(crate) async fn spawn(&mut self, mut spawnable: TokioCommand) -> std::io::Result<bool> {
		let command = match self {
			Self::IsRunning { .. } => {
				return Ok(false);
			}
			Self::ToRun(command) | Self::Finished { command, .. } => command,
		};

		#[cfg(test)]
		let child = super::TestChild::new(command.clone(), spawnable)?;

		#[cfg(not(test))]
		let child = if command.grouped {
			ErasedChild::Grouped(spawnable.group().spawn()?)
		} else {
			ErasedChild::Ungrouped(spawnable.spawn()?)
		};

		*self = Self::IsRunning {
			command: command.clone(),
			child,
			started: Instant::now(),
		};
		Ok(true)
	}

	#[must_use]
	pub(crate) fn reset(&mut self) -> Self {
		match self {
			Self::ToRun(command) => Self::ToRun(command.clone()),
			Self::Finished {
				command,
				status,
				started,
				finished,
			} => {
				let cloned = Self::Finished {
					command: command.clone(),
					status: *status,
					started: *started,
					finished: *finished,
				};

				*self = Self::ToRun(command.clone());
				cloned
			}
			Self::IsRunning {
				command, started, ..
			} => {
				let cloned = Self::Finished {
					command: command.clone(),
					status: ProcessEnd::Continued,
					started: *started,
					finished: Instant::now(),
				};

				*self = Self::ToRun(command.clone());
				cloned
			}
		}
	}
}
