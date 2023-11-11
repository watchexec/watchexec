use std::time::Instant;

use command_group::{tokio::ErasedChild, AsyncCommandGroup};
use tokio::process::Command;
use tracing::debug;
use watchexec_events::ProcessEnd;

use crate::{
	command::{Program, SequenceTree},
	errors::{sync_io_error, SyncIoError},
};

use super::task::SpawnHook;

#[derive(Debug)]
pub enum ProgramState {
	ToRun(Program),
	IsRunning {
		program: Program,
		child: ErasedChild,
		started: Instant,
	},
	FailedToStart {
		program: Program,
		error: SyncIoError,
		when: Instant,
	},
	Finished {
		program: Program,
		status: ProcessEnd,
		started: Instant,
		finished: Instant,
	},
}

pub type StateSequence = SequenceTree<ProgramState>;

impl From<crate::command::Sequence> for StateSequence {
	fn from(seq: crate::command::Sequence) -> Self {
		match seq {
			crate::command::Sequence::Run(program) => Self::Run(ProgramState::ToRun(program)),
			crate::command::Sequence::List(list) => {
				Self::List(list.into_iter().map(Self::from).collect())
			}
			crate::command::Sequence::Condition {
				given,
				then,
				otherwise,
			} => Self::Condition {
				given: Box::new(Self::from(*given)),
				then: then.map(|then| Box::new(Self::from(*then))),
				otherwise: otherwise.map(|otherwise| Box::new(Self::from(*otherwise))),
			},
		}
	}
}

impl StateSequence {
	pub(crate) fn current_program(&mut self) -> Option<&mut ProgramState> {
		match self {
			Self::Run(program @ ProgramState::IsRunning { .. }) => Some(program),
			Self::List(list) => list.iter_mut().find_map(|seq| seq.current_program()),
			Self::Condition {
				given,
				then,
				otherwise,
			} => given
				.current_program()
				.or_else(|| then.as_mut().and_then(|seq| seq.current_program()))
				.or_else(|| otherwise.as_mut().and_then(|seq| seq.current_program())),
			_ => None,
		}
	}

	pub(crate) fn current_child(&mut self) -> Option<&mut ErasedChild> {
		self.current_program().and_then(|program| match program {
			ProgramState::IsRunning { child, .. } => Some(child),
			_ => None,
		})
	}

	pub(crate) fn is_finished(&self) -> bool {
		match self {
			Self::Run(ProgramState::Finished { .. }) => true,
			Self::Run(_) => false,
			Self::List(list) => list.iter().all(|seq| seq.is_finished()),
			Self::Condition {
				given,
				then,
				otherwise,
			} => match (given.is_finished(), given.current_status()) {
				(false, _) => false,
				(true, Some(ProcessEnd::Success)) => {
					if let Some(then) = then {
						then.is_finished()
					} else {
						true
					}
				}
				(true, _) => {
					if let Some(otherwise) = otherwise {
						otherwise.is_finished()
					} else {
						true
					}
				}
			},
		}
	}

	pub(crate) fn current_status(&self) -> Option<ProcessEnd> {
		match self {
			Self::Run(ProgramState::Finished { status, .. }) => Some(*status),
			Self::Run(_) => None,
			Self::List(list) => list.iter().filter_map(|seq| seq.current_status()).last(),
			Self::Condition {
				given,
				then,
				otherwise,
			} => match (given.is_finished(), given.current_status()) {
				(false, status) => status,
				(true, None) => unreachable!("given is finished but has no status"),
				(true, status @ Some(ProcessEnd::Success)) => {
					if let Some(then) = then {
						then.current_status()
					} else {
						status
					}
				}
				(true, status) => {
					if let Some(otherwise) = otherwise {
						otherwise.current_status()
					} else {
						status
					}
				}
			},
		}
	}

	pub(crate) fn next_program_state(&mut self) -> Option<&mut ProgramState> {
		match self {
			Self::Run(program @ ProgramState::ToRun { .. }) => Some(program),
			Self::Run(_) => None,
			Self::List(list) => list.iter_mut().find_map(|seq| seq.next_program_state()),
			Self::Condition {
				given,
				then,
				otherwise,
			} => match (given.is_finished(), given.current_status()) {
				(false, _) => given.next_program_state(),
				(true, Some(ProcessEnd::Success)) => {
					if let Some(then) = then {
						then.next_program_state()
					} else {
						None
					}
				}
				(true, _) => {
					if let Some(otherwise) = otherwise {
						otherwise.next_program_state()
					} else {
						None
					}
				}
			},
		}
	}

	pub(crate) async fn spawn_next_program(&mut self, spawn_hook: &SpawnHook) -> SpawnResult {
		if let Some(ProgramState::IsRunning { .. }) = self.current_program() {
			return SpawnResult::AlreadyRunning;
		}

		if let Some(state) = self.next_program_state() {
			let ProgramState::ToRun(program) = state else {
				unreachable!("next_program_state() always returns ProgramState::ToRun");
			};
			let program = std::mem::replace(program, Program::empty());

			macro_rules! try_this {
				($erroring:expr) => {
					match $erroring {
						Ok(value) => value,
						Err(err) => {
							let err = sync_io_error(err);
							*state = ProgramState::FailedToStart {
								program,
								error: err.clone(),
								when: Instant::now(),
							};
							return SpawnResult::SpawnError(err);
						}
					}
				};
			}

			let mut command = program.to_spawnable();
			#[cfg(unix)]
			try_this!(reset_sigmask(&mut command));

			spawn_hook.call(&mut command, &program).await;

			let child = if matches!(program, Program::Exec { grouped: true, .. }) {
				ErasedChild::Grouped(try_this!(command.group().spawn()))
			} else {
				ErasedChild::Ungrouped(try_this!(command.spawn()))
			};

			*state = ProgramState::IsRunning {
				program,
				child,
				started: Instant::now(),
			};
			SpawnResult::Spawned
		} else {
			SpawnResult::SequenceFinished
		}
	}

	#[must_use]
	pub(crate) fn reset(&mut self) -> Self {
		match self {
			Self::Run(ProgramState::ToRun(program)) => {
				Self::Run(ProgramState::ToRun(program.clone()))
			}
			Self::Run(state @ ProgramState::FailedToStart { .. }) => {
				let (program, cloned) = {
					let ProgramState::FailedToStart {
						program,
						error,
						when,
					} = state
					else {
						unreachable!()
					};
					(
						program.clone(),
						ProgramState::FailedToStart {
							program: program.clone(),
							error: error.clone(),
							when: *when,
						},
					)
				};
				*state = ProgramState::ToRun(program.clone());
				Self::Run(cloned)
			}
			Self::Run(state @ ProgramState::Finished { .. }) => {
				let (program, cloned) = {
					let ProgramState::Finished {
						program,
						status,
						started,
						finished,
					} = state
					else {
						unreachable!()
					};
					(
						program.clone(),
						ProgramState::Finished {
							program: program.clone(),
							status: *status,
							started: *started,
							finished: *finished,
						},
					)
				};
				*state = ProgramState::ToRun(program.clone());
				Self::Run(cloned)
			}
			Self::Run(state @ ProgramState::IsRunning { .. }) => {
				let (program, cloned) = {
					let ProgramState::IsRunning {
						program, started, ..
					} = state
					else {
						unreachable!()
					};
					(
						program.clone(),
						ProgramState::Finished {
							program: program.clone(),
							status: ProcessEnd::Continued,
							started: *started,
							finished: Instant::now(),
						},
					)
				};
				*state = ProgramState::ToRun(program.clone());
				Self::Run(cloned)
			}
			Self::List(list) => Self::List(list.iter_mut().map(Self::reset).collect()),
			Self::Condition {
				given,
				then,
				otherwise,
			} => Self::Condition {
				given: Box::new(given.reset()),
				then: then.as_mut().map(|seq| Box::new(seq.reset())),
				otherwise: otherwise.as_mut().map(|seq| Box::new(seq.reset())),
			},
		}
	}
}

#[derive(Debug)]
pub(crate) enum SpawnResult {
	Spawned,
	SpawnError(SyncIoError),
	AlreadyRunning,
	SequenceFinished,
}

/// Resets the sigmask of the process before we spawn it.
///
/// Required from Rust 1.66:
/// https://github.com/rust-lang/rust/pull/101077
///
/// Done before the spawn hook so it can be used to set a different mask if desired.
#[cfg(unix)]
fn reset_sigmask(command: &mut Command) -> std::io::Result<()> {
	use nix::sys::signal::{sigprocmask, SigSet, SigmaskHow, Signal};
	unsafe {
		command.pre_exec(|| {
			let mut oldset = SigSet::empty();
			let mut newset = SigSet::all();
			newset.remove(Signal::SIGHUP); // leave SIGHUP alone so nohup works
			debug!(unblocking=?newset, "resetting process sigmask");
			sigprocmask(SigmaskHow::SIG_UNBLOCK, Some(&newset), Some(&mut oldset))?;
			debug!(?oldset, "sigmask reset");
			Ok(())
		});
	}

	Ok(())
}
