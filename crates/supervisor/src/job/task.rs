use std::{future::Future, sync::Arc};

use tokio::{process::Command as TokioCommand, task::JoinSet};
use watchexec_signals::Signal;

use crate::{
	command::Command,
	errors::{sync_io_error, SyncIoError},
	flag::Flag,
};

use super::{
	job::Job,
	messages::{Control, ControlMessage},
	state::CommandState,
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
		let mut command_state = CommandState::ToRun(command.clone());
		let mut previous_run = None;
		let mut on_end = Vec::new(); // TODO

		'main: while let Ok((ControlMessage { control, done }, _)) = receiver.recv().await {
			macro_rules! try_with_handler {
				($erroring:expr) => {
					match $erroring {
						Err(err) => {
							let fut = error_handler.call(sync_io_error(err));
							fut.await;
							done.raise();
							continue 'main;
						}
						Ok(value) => value,
					}
				};
			}

			match control {
				Control::Start => {
					let mut spawnable = command.to_spawnable();
					spawn_hook
						.call(
							&mut spawnable,
							&JobTaskContext {
								command: &command,
								current: &command_state,
								previous: previous_run.as_ref(),
							},
						)
						.await;
					try_with_handler!(command_state.spawn(spawnable).await);
				}
				//
				Control::Signal(signal) => {
					#[cfg(unix)]
					if let CommandState::IsRunning { child, .. } = &mut command_state {
						try_with_handler!(child.signal(
							signal
								.to_nix()
								.or_else(|| Signal::Terminate.to_nix())
								.unwrap()
						));
					}
				}
				Control::Delete => {
					done.raise();
					break 'main;
				}

				Control::NextEnding => {
					if !matches!(command_state, CommandState::Finished { .. }) {
						on_end.push(done);
						continue 'main;
					}
				}

				Control::SyncFunc(f) => {
					f(&JobTaskContext {
						command: &command,
						current: &command_state,
						previous: previous_run.as_ref(),
					});
				}
				Control::AsyncFunc(f) => {
					Box::into_pin(f(&JobTaskContext {
						command: &command,
						current: &command_state,
						previous: previous_run.as_ref(),
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

#[derive(Debug)]
pub struct JobTaskContext<'task> {
	pub command: &'task Command,
	pub current: &'task CommandState,
	pub previous: Option<&'task CommandState>,
}

pub(crate) type SyncFunc = Box<dyn FnOnce(&JobTaskContext) + Send + Sync + 'static>;
pub(crate) type AsyncFunc = Box<
	dyn (FnOnce(&JobTaskContext) -> Box<dyn Future<Output = ()> + Send + Sync>)
		+ Send
		+ Sync
		+ 'static,
>;

pub(crate) type SyncSpawnHook =
	Arc<dyn Fn(&mut TokioCommand, &JobTaskContext) + Send + Sync + 'static>;
pub(crate) type AsyncSpawnHook = Arc<
	dyn (Fn(&mut TokioCommand, &JobTaskContext) -> Box<dyn Future<Output = ()> + Send + Sync>)
		+ Send
		+ Sync
		+ 'static,
>;

sync_async_callbox!(SpawnHook, SyncSpawnHook, AsyncSpawnHook, (command: &mut TokioCommand, context: &JobTaskContext<'_>));

pub(crate) type SyncErrorHandler = Arc<dyn Fn(SyncIoError) + Send + Sync + 'static>;
pub(crate) type AsyncErrorHandler = Arc<
	dyn (Fn(SyncIoError) -> Box<dyn Future<Output = ()> + Send + Sync>) + Send + Sync + 'static,
>;

sync_async_callbox!(ErrorHandler, SyncErrorHandler, AsyncErrorHandler, (error: SyncIoError));
