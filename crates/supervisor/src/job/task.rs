use std::{future::Future, sync::Arc, time::Instant};

use tokio::{process::Command as TokioCommand, task::JoinSet};
use watchexec_signals::Signal;

use crate::{
	command::Command,
	errors::{sync_io_error, SyncIoError},
	flag::Flag,
	job::priority::Timer,
};

use super::{
	job::Job,
	messages::{Control, ControlMessage},
	priority,
	state::CommandState,
};

pub fn start_job(joinset: &mut JoinSet<()>, command: Command) -> Job {
	let (sender, mut receiver) = priority::new();

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
		let mut command_state = CommandState::ToRun;
		let mut previous_run = None;
		let mut stop_timer = None;
		let mut on_end: Vec<Flag> = Vec::new();

		'main: while let Some(ControlMessage { control, done }) =
			receiver.recv(&mut stop_timer).await
		{
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

			eprintln!("[{:?}] control: {control:?}", Instant::now());

			match control {
				Control::Start => {
					let mut spawnable = command.to_spawnable();
					previous_run = Some(command_state.reset());
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
					try_with_handler!(command_state.spawn(command.clone(), spawnable).await);
				}
				Control::Stop => {
							if let CommandState::Running { child, started, .. } = &mut command_state {
						try_with_handler!(child.kill().await);
						let status = try_with_handler!(child.wait().await);

						command_state = CommandState::Finished {
							status: status.into(),
							started: *started,
							finished: Instant::now(),
						};

						for done in on_end.drain(..) {
							done.raise();
						}
					}
				}
				Control::GracefulStop { signal, grace } => {
							if let CommandState::Running { child, .. } = &mut command_state {
						try_with_handler!(signal_child(signal, child).await);

						stop_timer.replace(Timer::stop(grace, done));
						continue 'main;
					}
				}
				Control::TryRestart => {
							if let CommandState::Running { child, started, .. } = &mut command_state {
						try_with_handler!(child.kill().await);
						let status = try_with_handler!(child.wait().await);

						command_state = CommandState::Finished {
							status: status.into(),
							started: *started,
							finished: Instant::now(),
						};
						previous_run = Some(command_state.reset());

						for done in on_end.drain(..) {
							done.raise();
						}

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
						try_with_handler!(command_state.spawn(command.clone(), spawnable).await);
					}
				}
				Control::TryGracefulRestart { signal, grace } => {
							if let CommandState::Running { child, .. } = &mut command_state {
						try_with_handler!(signal_child(signal, child).await);

						stop_timer.replace(Timer::restart(grace, done));
						continue 'main;
					}
				}
				Control::ContinueTryGracefulRestart => {
							if let CommandState::Running { child, started, .. } = &mut command_state {
						try_with_handler!(child.kill().await);
						let status = try_with_handler!(child.wait().await);

						command_state = CommandState::Finished {
							status: status.into(),
							started: *started,
							finished: Instant::now(),
						};

						for done in on_end.drain(..) {
							done.raise();
						}
					}

					let mut spawnable = command.to_spawnable();
					previous_run = Some(command_state.reset());
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
					try_with_handler!(command_state.spawn(command.clone(), spawnable).await);
				}
				Control::Signal(signal) => {
							if let CommandState::Running { child, .. } = &mut command_state {
						try_with_handler!(signal_child(signal, child).await);
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

async fn signal_child(
	signal: Signal,
	#[cfg(test)] child: &mut super::TestChild,
	#[cfg(not(test))] child: &mut command_group::tokio::ErasedChild,
) -> std::io::Result<()> {
	#[cfg(unix)]
	child.signal(
		signal
			.to_nix()
			.or_else(|| Signal::Terminate.to_nix())
			.unwrap(),
	)?;

	#[cfg(windows)]
	if signal == Signal::ForceStop {
		child.start_kill().await?;
	}

	Ok(())
}
