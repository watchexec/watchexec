use std::{future::Future, sync::Arc};

use tokio::{process::Command as TokioCommand, task::JoinSet};

use crate::{
	command::{Command, Program},
	errors::{sync_io_error, SyncIoError},
	flag::Flag,
	job::program_state::SpawnResult,
};

use super::{
	job::Job,
	messages::{Control, ControlMessage},
	program_state::StateSequence,
};

pub fn start_job(joinset: &mut JoinSet<()>, command: Command, channel_size: Option<usize>) -> Job {
	let (sender, receiver) = channel_size.map_or_else(
		async_priority_channel::unbounded,
		async_priority_channel::bounded,
	);

	let gone = Flag::default();
	let done = gone.clone();

	let job = Job {
		command: Arc::new(command.clone()),
		control_queue: sender,
		gone,
	};

	joinset.spawn(async move {
		let mut error_handler = ErrorHandler::None;
		let mut spawn_hook = SpawnHook::None;
		let mut sequence = StateSequence::from(command.sequence);
		let mut last_sequence = None;
		let mut when_program_ends = Vec::new();
		let mut when_sequence_ends = Vec::new();

		'main: while let Ok((ControlMessage { control, done }, _)) = receiver.recv().await {
			macro_rules! try_with_handler {
				($erroring:expr) => {
					if let Err(err) = $erroring {
						let fut = error_handler.call(sync_io_error(err));
						fut.await;
						done.raise();
						continue 'main;
					}
				};
			}

			match control {
				Control::Start => 'start: loop {
					match sequence.spawn_next_program(&spawn_hook).await {
						SpawnResult::Spawned | SpawnResult::AlreadyRunning => break 'start,
						SpawnResult::SpawnError(error) => {
							error_handler.call(error).await;
							break 'start;
						}
						SpawnResult::SequenceFinished => {
							last_sequence = Some(sequence.reset());
							continue 'start;
						}
					}
				},
				//
				Control::Signal(signal) => {
					if let Some(child) = sequence.current_child() {
						try_with_handler!(child.signal(signal));
					}
				}
				Control::Delete => {
					done.raise();
					break 'main;
				}

				Control::NextEnding => {
					if sequence.is_finished() {
						done.raise();
					} else {
						when_program_ends.push(done);
					}
					continue 'main;
				}
				Control::SequenceEnding => {
					if sequence.is_finished() {
						done.raise();
					} else {
						when_sequence_ends.push(done);
					}
					continue 'main;
				}

				Control::SyncFunc(f) => {
					f(&JobTaskContext {
						current_sequence: &sequence,
						last_sequence: last_sequence.as_ref(),
					});
				}
				Control::AsyncFunc(f) => {
					Box::into_pin(f(&JobTaskContext {
						current_sequence: &sequence,
						last_sequence: last_sequence.as_ref(),
					}))
					.await;
				}

				Control::SetSyncErrorHandler(f) => {
					error_handler = ErrorHandler::Sync(f);
				}
				Control::SetAsyncErrorHandler(f) => {
					error_handler = ErrorHandler::Async(f);
				}
				Control::UnsetErrorHandler => {
					error_handler = ErrorHandler::None;
				}
				Control::SetSyncSpawnHook(f) => {
					spawn_hook = SpawnHook::Sync(f);
				}
				Control::SetAsyncSpawnHook(f) => {
					spawn_hook = SpawnHook::Async(f);
				}
				Control::UnsetSpawnHook => {
					spawn_hook = SpawnHook::None;
				}
				_ => todo!(),
			}

			done.raise();
		}

		done.raise();
	});

	job
}

macro_rules! sync_async_callbox {
	($name:ident, $synct:ty, $asynct:ty, ($($argname:ident : $argtype:ty),*)) => {
		pub(crate) enum $name {
			None,
			Sync($synct),
			Async($asynct),
		}

		impl $name {
			pub async fn call(&self, $($argname: $argtype),*) {
				match self {
					$name::None => (),
					$name::Sync(f) => f($($argname),*),
					$name::Async(f) => Box::into_pin(f($($argname),*)).await,
				}
			}
		}
	};
}

pub struct JobTaskContext<'task> {
	pub current_sequence: &'task StateSequence,
	pub last_sequence: Option<&'task StateSequence>,
}

pub(crate) type SyncFunc = Box<dyn FnOnce(&JobTaskContext) + Send + Sync + 'static>;
pub(crate) type AsyncFunc = Box<
	dyn (FnOnce(&JobTaskContext) -> Box<dyn Future<Output = ()> + Send + Sync>)
		+ Send
		+ Sync
		+ 'static,
>;

pub(crate) type SyncSpawnHook = Arc<dyn Fn(&mut TokioCommand, &Program) + Send + Sync + 'static>;
pub(crate) type AsyncSpawnHook = Arc<
	dyn (Fn(&mut TokioCommand, &Program) -> Box<dyn Future<Output = ()> + Send + Sync>)
		+ Send
		+ Sync
		+ 'static,
>;

sync_async_callbox!(SpawnHook, SyncSpawnHook, AsyncSpawnHook, (command: &mut TokioCommand, program: &Program));

pub(crate) type SyncErrorHandler = Arc<dyn Fn(SyncIoError) + Send + Sync + 'static>;
pub(crate) type AsyncErrorHandler = Arc<
	dyn (Fn(SyncIoError) -> Box<dyn Future<Output = ()> + Send + Sync>) + Send + Sync + 'static,
>;

sync_async_callbox!(ErrorHandler, SyncErrorHandler, AsyncErrorHandler, (error: SyncIoError));
