use std::{future::Future, mem::take, sync::Arc, time::Instant};

use process_wrap::tokio::TokioCommandWrap;
use tokio::{select, task::JoinHandle};
use tracing::{instrument, trace, trace_span, Instrument};
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

/// Spawn a job task and return a [`Job`] handle and a [`JoinHandle`].
///
/// The job task immediately starts in the background: it does not need polling.
#[must_use]
#[instrument(level = "trace")]
pub fn start_job(command: Arc<Command>) -> (Job, JoinHandle<()>) {
	enum Loop {
		Normally,
		Skip,
		Break,
	}

	let (sender, mut receiver) = priority::new();
	let gone = Flag::default();
	let done = gone.clone();

	(
		Job {
			command: command.clone(),
			control_queue: sender,
			gone,
		},
		tokio::spawn(async move {
			let mut error_handler = ErrorHandler::None;
			let mut spawn_hook = SpawnHook::None;
			let mut command_state = CommandState::Pending;
			let mut previous_run = None;
			let mut stop_timer = None;
			let mut on_end: Vec<Flag> = Vec::new();
			let mut on_end_restart: Option<Flag> = None;

			'main: loop {
				select! {
					result = command_state.wait(), if command_state.is_running() => {
						trace!(?result, ?command_state, "got wait result");
						match async {
							#[cfg(test)] eprintln!("[{:?}] waited: {result:?}", Instant::now());

							match result {
								Err(err) => {
									let fut = error_handler.call(sync_io_error(err));
									fut.await;
									return Loop::Skip;
								}
								Ok(true) => {
									trace!(existing=?stop_timer, "erasing stop timer");
									stop_timer = None;
									trace!(count=%on_end.len(), "raising all pending end flags");
									for done in take(&mut on_end) {
										done.raise();
									}

									if let Some(flag) = on_end_restart.take() {
										trace!("continuing a graceful restart");

										let mut spawnable = command.to_spawnable();
										previous_run = Some(command_state.reset());
										spawn_hook
											.call(
												&mut spawnable,
												&JobTaskContext {
													command: command.clone(),
													current: &command_state,
													previous: previous_run.as_ref(),
												},
											)
											.await;
										if let Err(err) = command_state.spawn(command.clone(), spawnable) {
											let fut = error_handler.call(sync_io_error(err));
											fut.await;
											return Loop::Skip;
										}

										trace!("raising graceful restart's flag");
										flag.raise();
									}
								}
								Ok(false) => {
									trace!("child wasn't running, ignoring wait result");
								}
							}

							Loop::Normally
						}.instrument(trace_span!("handle wait result")).await {
							Loop::Normally => {}
							Loop::Skip => {
								trace!("skipping to next event");
								continue 'main;
							}
							Loop::Break => {
								trace!("breaking out of main loop");
								break 'main;
							}
						}
					}
					Some(ControlMessage { control, done }) = receiver.recv(&mut stop_timer) => {
						match async {
							trace!(?control, ?command_state, "got control message");
							#[cfg(test)] eprintln!("[{:?}] control: {control:?}", Instant::now());

							macro_rules! try_with_handler {
								($erroring:expr) => {
									match $erroring {
										Err(err) => {
											let fut = error_handler.call(sync_io_error(err));
											fut.await;
											trace!("raising done flag for this control after error");
											done.raise();
											return Loop::Normally;
										}
										Ok(value) => value,
									}
								};
							}

							match control {
								Control::Start => {
									if command_state.is_running() {
										trace!("child is running, skip");
									} else {
										let mut spawnable = command.to_spawnable();
										previous_run = Some(command_state.reset());
										spawn_hook
											.call(
												&mut spawnable,
												&JobTaskContext {
													command: command.clone(),
													current: &command_state,
													previous: previous_run.as_ref(),
												},
											)
											.await;
										try_with_handler!(command_state.spawn(command.clone(), spawnable));
									}
								}
								Control::Stop => {
									if let CommandState::Running { child, started, .. } = &mut command_state {
										trace!("stopping child");
										try_with_handler!(Box::into_pin(child.kill()).await);
										trace!("waiting on child");
										let status = try_with_handler!(Box::into_pin(child.wait()).await);

										trace!(?status, "got child end status");
										command_state = CommandState::Finished {
											status: status.into(),
											started: *started,
											finished: Instant::now(),
										};

										trace!(count=%on_end.len(), "raising all pending end flags");
										for done in take(&mut on_end) {
											done.raise();
										}
									} else {
										trace!("child isn't running, skip");
									}
								}
								Control::GracefulStop { signal, grace } => {
									if let CommandState::Running { child, .. } = &mut command_state {
										try_with_handler!(signal_child(signal, child).await);

										trace!(?grace, "setting up graceful stop timer");
										stop_timer.replace(Timer::stop(grace, done));
										return Loop::Skip;
									}
									trace!("child isn't running, skip");
								}
								Control::TryRestart => {
									if let CommandState::Running { child, started, .. } = &mut command_state {
										trace!("stopping child");
										try_with_handler!(Box::into_pin(child.kill()).await);
										trace!("waiting on child");
										let status = try_with_handler!(Box::into_pin(child.wait()).await);

										trace!(?status, "got child end status");
										command_state = CommandState::Finished {
											status: status.into(),
											started: *started,
											finished: Instant::now(),
										};
										previous_run = Some(command_state.reset());

										trace!(count=%on_end.len(), "raising all pending end flags");
										for done in take(&mut on_end) {
											done.raise();
										}

										let mut spawnable = command.to_spawnable();
										spawn_hook
											.call(
												&mut spawnable,
												&JobTaskContext {
													command: command.clone(),
													current: &command_state,
													previous: previous_run.as_ref(),
												},
											)
											.await;
										try_with_handler!(command_state.spawn(command.clone(), spawnable));
									} else {
										trace!("child isn't running, skip");
									}
								}
								Control::TryGracefulRestart { signal, grace } => {
									if let CommandState::Running { child, .. } = &mut command_state {
										try_with_handler!(signal_child(signal, child).await);

										trace!(?grace, "setting up graceful stop timer");
										stop_timer.replace(Timer::restart(grace, done.clone()));
										trace!("setting up graceful restart flag");
										on_end_restart = Some(done);
										return Loop::Skip;
									}
									trace!("child isn't running, skip");
								}
								Control::ContinueTryGracefulRestart => {
									trace!("continuing a graceful try-restart");

									if let CommandState::Running { child, started, .. } = &mut command_state {
										trace!("stopping child forcefully");
										try_with_handler!(Box::into_pin(child.kill()).await);
										trace!("waiting on child");
										let status = try_with_handler!(Box::into_pin(child.wait()).await);

										trace!(?status, "got child end status");
										command_state = CommandState::Finished {
											status: status.into(),
											started: *started,
											finished: Instant::now(),
										};

										trace!(count=%on_end.len(), "raising all pending end flags");
										for done in take(&mut on_end) {
											done.raise();
										}
									}

									let mut spawnable = command.to_spawnable();
									previous_run = Some(command_state.reset());
									spawn_hook
										.call(
											&mut spawnable,
											&JobTaskContext {
												command: command.clone(),
												current: &command_state,
												previous: previous_run.as_ref(),
											},
										)
										.await;
									try_with_handler!(command_state.spawn(command.clone(), spawnable));
								}
								Control::Signal(signal) => {
									if let CommandState::Running { child, .. } = &mut command_state {
										try_with_handler!(signal_child(signal, child).await);
									} else {
										trace!("child isn't running, skip");
									}
								}
								Control::Delete => {
									trace!("raising done flag immediately");
									done.raise();
									return Loop::Break;
								}

								Control::NextEnding => {
									if matches!(command_state, CommandState::Finished { .. }) {
										trace!("child is finished, raise done flag immediately");
										done.raise();
										return Loop::Normally;
									}
										trace!("queue end flag");
										on_end.push(done);
										return Loop::Skip;
								}

								Control::SyncFunc(f) => {
									f(&JobTaskContext {
										command: command.clone(),
										current: &command_state,
										previous: previous_run.as_ref(),
									});
								}
								Control::AsyncFunc(f) => {
									Box::into_pin(f(&JobTaskContext {
										command: command.clone(),
										current: &command_state,
										previous: previous_run.as_ref(),
									}))
									.await;
								}

								Control::SetSyncErrorHandler(f) => {
									trace!("setting sync error handler");
									error_handler = ErrorHandler::Sync(f);
								}
								Control::SetAsyncErrorHandler(f) => {
									trace!("setting async error handler");
									error_handler = ErrorHandler::Async(f);
								}
								Control::UnsetErrorHandler => {
									trace!("unsetting error handler");
									error_handler = ErrorHandler::None;
								}
								Control::SetSyncSpawnHook(f) => {
									trace!("setting sync spawn hook");
									spawn_hook = SpawnHook::Sync(f);
								}
								Control::SetAsyncSpawnHook(f) => {
									trace!("setting async spawn hook");
									spawn_hook = SpawnHook::Async(f);
								}
								Control::UnsetSpawnHook => {
									trace!("unsetting spawn hook");
									spawn_hook = SpawnHook::None;
								}
							}

							trace!("raising control done flag");
							done.raise();

							Loop::Normally
					}.instrument(trace_span!("handle control message")).await {
						Loop::Normally => {}
						Loop::Skip => {
							trace!("skipping to next event (without raising done flag)");
							continue 'main;
						}
						Loop::Break => {
							trace!("breaking out of main loop");
							break 'main;
						}
					}
				}
				}
			}

			trace!("raising job done flag");
			done.raise();
		}),
	)
}

macro_rules! sync_async_callbox {
	($name:ident, $synct:ty, $asynct:ty, ($($argname:ident : $argtype:ty),*)) => {
		pub enum $name {
			None,
			Sync($synct),
			Async($asynct),
		}

		impl $name {
			#[instrument(level = "trace", skip(self, $($argname),*))]
			pub async fn call(&self, $($argname: $argtype),*) {
				match self {
					$name::None => (),
					$name::Sync(f) => {
						::tracing::trace!("calling sync {:?}", stringify!($name));
						f($($argname),*)
					}
					$name::Async(f) => {
						::tracing::trace!("calling async {:?}", stringify!($name));
						Box::into_pin(f($($argname),*)).await
					}
				}
			}
		}
	};
}

/// Job task internals exposed via hooks.
#[derive(Debug)]
pub struct JobTaskContext<'task> {
	/// The job's [`Command`].
	pub command: Arc<Command>,

	/// The current state of the job.
	pub current: &'task CommandState,

	/// The state of the previous iteration of the job, if any.
	///
	/// This is generally [`CommandState::Finished`], but may be other states in rare cases.
	pub previous: Option<&'task CommandState>,
}

pub type SyncFunc = Box<dyn FnOnce(&JobTaskContext<'_>) + Send + Sync + 'static>;
pub type AsyncFunc = Box<
	dyn (FnOnce(&JobTaskContext<'_>) -> Box<dyn Future<Output = ()> + Send + Sync>)
		+ Send
		+ Sync
		+ 'static,
>;

pub type SyncSpawnHook =
	Arc<dyn Fn(&mut TokioCommandWrap, &JobTaskContext<'_>) + Send + Sync + 'static>;
pub type AsyncSpawnHook = Arc<
	dyn (Fn(&mut TokioCommandWrap, &JobTaskContext<'_>) -> Box<dyn Future<Output = ()> + Send + Sync>)
		+ Send
		+ Sync
		+ 'static,
>;

sync_async_callbox!(SpawnHook, SyncSpawnHook, AsyncSpawnHook, (command: &mut TokioCommandWrap, context: &JobTaskContext<'_>));

pub type SyncErrorHandler = Arc<dyn Fn(SyncIoError) + Send + Sync + 'static>;
pub type AsyncErrorHandler = Arc<
	dyn (Fn(SyncIoError) -> Box<dyn Future<Output = ()> + Send + Sync>) + Send + Sync + 'static,
>;

sync_async_callbox!(ErrorHandler, SyncErrorHandler, AsyncErrorHandler, (error: SyncIoError));

#[cfg_attr(not(windows), allow(clippy::needless_pass_by_ref_mut))] // needed for start_kill()
#[instrument(level = "trace")]
async fn signal_child(
	signal: Signal,
	child: &mut Box<dyn process_wrap::tokio::TokioChildWrapper>,
) -> std::io::Result<()> {
	#[cfg(unix)]
	{
		let sig = signal
			.to_nix()
			.or_else(|| Signal::Terminate.to_nix())
			.expect("UNWRAP: guaranteed for Signal::Terminate default");
		trace!(signal=?sig, "sending signal");
		child.signal(sig as _)?;
	}

	#[cfg(windows)]
	if signal == Signal::ForceStop {
		trace!("starting kill, without waiting");
		child.start_kill()?;
	} else {
		trace!(?signal, "ignoring unsupported signal");
	}

	Ok(())
}
