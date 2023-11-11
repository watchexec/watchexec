use std::{
	future::Future,
	sync::{Arc, OnceLock},
};

use tokio::{process::Command as TokioCommand, task::JoinSet};

use crate::{
	command::{Command, Program},
	flag::Flag,
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

		while let Ok((ControlMessage { control, done }, _)) = receiver.recv().await {
			macro_rules! try_with_handler {
				($erroring:expr) => {
					if let Err(err) = $erroring {
						let lock = OnceLock::new();
						lock.set(err).ok();
						let fut = error_handler.call(lock);
						fut.await;
						done.raise();
						continue;
					}
				};
			}

			match control {
				Control::SetAsyncErrorHandler(f) => {
					error_handler = ErrorHandler::Async(f);
				}
				Control::SetErrorHandler(f) => {
					error_handler = ErrorHandler::Sync(f);
				}
				Control::UnsetErrorHandler => {
					error_handler = ErrorHandler::None;
				}
				Control::SetAsyncSpawnHook(f) => {
					spawn_hook = SpawnHook::Async(f);
				}
				Control::SetSpawnHook(f) => {
					spawn_hook = SpawnHook::Sync(f);
				}
				Control::UnsetSpawnHook => {
					spawn_hook = SpawnHook::None;
				}

				Control::AsyncFunc(f) => {
					Box::into_pin(f()).await;
				}
				Control::Func(f) => {
					f();
				}

				Control::Signal(signal) => {
					if let Some(child) = sequence.current_child() {
						try_with_handler!(child.signal(signal));
					}
				}

				Control::Delete => {
					done.raise();
					break;
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
		enum $name {
			None,
			Sync($synct),
			Async($asynct),
		}

		impl $name {
			async fn call(&self, $($argname: $argtype),*) {
				match self {
					$name::None => (),
					$name::Sync(f) => f($($argname),*),
					$name::Async(f) => Box::into_pin(f($($argname),*)).await,
				}
			}
		}
	};
}

pub(crate) type SyncSpawnHook = Arc<dyn Fn(&mut TokioCommand, &Program) + Send + Sync + 'static>;
pub(crate) type AsyncSpawnHook = Arc<
	dyn (Fn(&mut TokioCommand, &Program) -> Box<dyn Future<Output = ()> + Send + Sync>)
		+ Send
		+ Sync
		+ 'static,
>;

sync_async_callbox!(SpawnHook, SyncSpawnHook, AsyncSpawnHook, (command: &mut TokioCommand, program: &Program));

pub type SyncIoError = OnceLock<std::io::Error>;
pub(crate) type SyncErrorHandler = Arc<dyn Fn(SyncIoError) + Send + Sync + 'static>;
pub(crate) type AsyncErrorHandler = Arc<
	dyn (Fn(SyncIoError) -> Box<dyn Future<Output = ()> + Send + Sync>) + Send + Sync + 'static,
>;

sync_async_callbox!(ErrorHandler, SyncErrorHandler, AsyncErrorHandler, (error: SyncIoError));
