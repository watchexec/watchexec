use std::{
	future::Future,
	mem::take,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc, Mutex,
	},
	time::{Duration, Instant},
};

#[cfg(unix)]
use nix::sys::signal::Signal as NixSignal;
#[cfg(unix)]
use nix::unistd::Pid;
use process_wrap::tokio::CommandWrap;
use tokio::{select, task::JoinHandle};
use tracing::{debug, instrument, trace, trace_span, Instrument};
use watchexec_signals::Signal;

use crate::{
	command::Command,
	errors::{sync_io_error, SyncIoError},
	flag::Flag,
	job::priority::Timer,
};

#[cfg(unix)]
use crate::foreground::ForegroundGrant;

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
	#[cfg_attr(test, allow(unused_variables))]
	let (pause_tx, mut pause_rx) = tokio::sync::mpsc::unbounded_channel::<i32>();
	#[cfg(not(unix))]
	drop(pause_tx); // no terminal pause watching on this platform: close the channel
	let gone = Flag::default();
	let done = gone.clone();
	let running = Arc::new(AtomicBool::new(false));
	let running_flag = running.clone();
	let spawner = Arc::new(SpawnerSlot::default());
	let job_spawner = Arc::clone(&spawner);

	(
		Job {
			command: command.clone(),
			control_queue: sender,
			gone,
			running,
			spawner: job_spawner,
		},
		tokio::spawn(async move {
			let mut error_handler = ErrorHandler::None;
			let mut spawn_hook = SpawnHook::None;
			let mut command_state = CommandState::Pending;
			let mut previous_run = None;
			let mut stop_timer = None;
			let mut on_end: Vec<Flag> = Vec::new();
			let mut on_end_restart: Option<Flag> = None;
			#[cfg(unix)]
			let mut foreground_grant: Option<ForegroundGrant> = None;
			#[cfg(unix)]
			let mut pause_watch: Option<Flag> = None;

			'main: loop {
				running_flag.store(command_state.is_running(), Ordering::Relaxed);
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
									#[cfg(unix)]
									{
										end_pause_watch(&mut pause_watch);
										if let Some(grant) = foreground_grant.take() {
											trace!("reclaiming terminal foreground (command exited)");
											grant.release();
										}
									}

									trace!(existing=?stop_timer, "erasing stop timer");
									if let Some(timer) = stop_timer.take() {
										timer.done.raise();
									}
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
										if let Err(err) = command_state.spawn(
											command.clone(),
											spawnable,
											&spawner,
										) {
											let fut = error_handler.call(sync_io_error(err));
											fut.await;
											return Loop::Skip;
										}

										#[cfg(all(unix, not(test)))]
										start_pause_watch_if_running(
											&command_state,
											&command,
											&mut pause_watch,
											pause_tx.clone(),
										);

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
					Some(raw_signal) = pause_rx.recv(), if command_state.is_running() => {
						#[cfg(unix)]
						handle_pause_event(
							raw_signal,
							&command,
							&mut command_state,
							&mut foreground_grant,
						)
						.instrument(trace_span!("handle pause event"))
						.await;
						#[cfg(not(unix))]
						drop(raw_signal);
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
										try_with_handler!(command_state.spawn(
											command.clone(),
											spawnable,
											&spawner,
										));
										#[cfg(all(unix, not(test)))]
										start_pause_watch_if_running(
											&command_state,
											&command,
											&mut pause_watch,
											pause_tx.clone(),
										);
									}
								}
								Control::Stop => {
									#[cfg(unix)]
									{
										end_pause_watch(&mut pause_watch);
										if let Some(grant) = foreground_grant.take() {
											trace!("reclaiming terminal foreground (stopping command)");
											grant.release();
										}
									}

									if let CommandState::Running { child, started, .. } = &mut command_state {
										trace!("stopping child");
										try_with_handler!(Box::into_pin(child.kill()).await);
										trace!("waiting on child");
										let status = try_with_handler!(child.wait().await);

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
									#[cfg(unix)]
									{
										end_pause_watch(&mut pause_watch);
										if let Some(grant) = foreground_grant.take() {
											trace!("reclaiming terminal foreground (restarting command)");
											grant.release();
										}
									}

									if let CommandState::Running { child, started, .. } = &mut command_state {
										trace!("stopping child");
										try_with_handler!(Box::into_pin(child.kill()).await);
										trace!("waiting on child");
										let status = try_with_handler!(child.wait().await);

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
										try_with_handler!(command_state.spawn(
											command.clone(),
											spawnable,
											&spawner,
										));
										#[cfg(all(unix, not(test)))]
										start_pause_watch_if_running(
											&command_state,
											&command,
											&mut pause_watch,
											pause_tx.clone(),
										);
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

									#[cfg(unix)]
									{
										end_pause_watch(&mut pause_watch);
										if let Some(grant) = foreground_grant.take() {
											trace!("reclaiming terminal foreground (restarting command)");
											grant.release();
										}
									}

									if let CommandState::Running { child, started, .. } = &mut command_state {
										trace!("stopping child forcefully");
										try_with_handler!(Box::into_pin(child.kill()).await);
										trace!("waiting on child");
										let status = try_with_handler!(child.wait().await);

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
									try_with_handler!(command_state.spawn(
										command.clone(),
										spawnable,
										&spawner,
									));
									#[cfg(all(unix, not(test)))]
									start_pause_watch_if_running(
										&command_state,
										&command,
										&mut pause_watch,
										pause_tx.clone(),
									);
								}
								Control::Signal(signal) => {
									if let CommandState::Running { child, .. } = &mut command_state {
										try_with_handler!(signal_child(signal, child).await);
									} else {
										trace!("child isn't running, skip");
									}
								}
								Control::Delete => {
									#[cfg(unix)]
									{
										end_pause_watch(&mut pause_watch);
										if let Some(grant) = foreground_grant.take() {
											grant.release();
										}
									}

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
								Control::SetSpawnFn(f) => {
									trace!("setting spawn fn");
									spawner.set(Spawner::Command(f));
								}
								Control::ClearSpawnFn => {
									trace!("clearing spawn fn");
									spawner.set(Spawner::Default);
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
				else => {
					trace!("all select branches disabled, exiting");
					break 'main;
				}
				}
			}

			#[cfg(unix)]
			{
				end_pause_watch(&mut pause_watch);
				if let Some(grant) = foreground_grant.take() {
					grant.release();
				}
			}

			trace!("raising job done flag");
			running_flag.store(false, Ordering::Relaxed);
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

pub type SyncSpawnHook = Arc<dyn Fn(&mut CommandWrap, &JobTaskContext<'_>) + Send + Sync + 'static>;
pub type AsyncSpawnHook = Arc<
	dyn (Fn(&mut CommandWrap, &JobTaskContext<'_>) -> Box<dyn Future<Output = ()> + Send + Sync>)
		+ Send
		+ Sync
		+ 'static,
>;

/// A function that customises how the underlying process is spawned.
///
/// When set on a [`Job`](super::Job), this function is passed to
/// [`CommandWrap::spawn_with()`](process_wrap::tokio::CommandWrap::spawn_with) instead of using
/// the default [`CommandWrap::spawn()`](process_wrap::tokio::CommandWrap::spawn). It receives a
/// `&mut tokio::process::Command` and must return the spawned `tokio::process::Child`.
///
/// All process-wrap layers are still applied around the child, so this only customises the
/// low-level spawn step. This is useful for delegating process spawning to a privileged helper
/// (e.g. for Linux capability granting) while keeping the supervisor's lifecycle management.
pub type SpawnFn = Arc<
	dyn Fn(&mut tokio::process::Command) -> std::io::Result<tokio::process::Child>
		+ Send
		+ Sync
		+ 'static,
>;

/// A function that replaces the normal process spawn and returns a supervised child.
///
/// Unlike [`SpawnFn`], this receives ownership of the prepared [`CommandWrap`] and returns an
/// arbitrary [`ChildWrapper`](process_wrap::tokio::ChildWrapper). This supports processes created
/// by an external mechanism, such as a privileged launcher, which cannot return a
/// [`tokio::process::Child`].
///
/// Spawn hooks have already run before this function is called. The function owns the
/// `CommandWrap`, so it is also responsible for spawning it or otherwise handling its configured
/// process-wrap layers.
pub type SpawnChildFn = Arc<
	dyn Fn(CommandWrap) -> std::io::Result<Box<dyn process_wrap::tokio::ChildWrapper>>
		+ Send
		+ Sync
		+ 'static,
>;

#[derive(Clone)]
#[cfg_attr(test, allow(dead_code))]
pub(super) enum Spawner {
	Default,
	Command(SpawnFn),
	Child(SpawnChildFn),
}

pub(super) struct SpawnerSlot(Mutex<Spawner>);

impl SpawnerSlot {
	pub(super) fn get(&self) -> Spawner {
		self.0.lock().unwrap_or_else(|err| err.into_inner()).clone()
	}

	pub(super) fn set(&self, spawner: Spawner) {
		*self.0.lock().unwrap_or_else(|err| err.into_inner()) = spawner;
	}
}

impl Default for SpawnerSlot {
	fn default() -> Self {
		Self(Mutex::new(Spawner::Default))
	}
}

impl std::fmt::Debug for SpawnerSlot {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let name = match self.get() {
			Spawner::Default => "default",
			Spawner::Command(_) => "command",
			Spawner::Child(_) => "child",
		};
		f.debug_tuple("SpawnerSlot").field(&name).finish()
	}
}

sync_async_callbox!(SpawnHook, SyncSpawnHook, AsyncSpawnHook, (command: &mut CommandWrap, context: &JobTaskContext<'_>));

pub type SyncErrorHandler = Arc<dyn Fn(SyncIoError) + Send + Sync + 'static>;
pub type AsyncErrorHandler = Arc<
	dyn (Fn(SyncIoError) -> Box<dyn Future<Output = ()> + Send + Sync>) + Send + Sync + 'static,
>;

sync_async_callbox!(ErrorHandler, SyncErrorHandler, AsyncErrorHandler, (error: SyncIoError));

/// Handle a pause notification from the terminal pause watcher.
#[cfg(unix)]
#[instrument(level = "trace", skip_all, fields(%raw_signal))]
async fn handle_pause_event(
	raw_signal: i32,
	command: &Command,
	command_state: &mut CommandState,
	foreground_grant: &mut Option<ForegroundGrant>,
) {
	let nix_signal = NixSignal::try_from(raw_signal).ok();
	let tty_denied = matches!(
		nix_signal,
		Some(NixSignal::SIGTTIN) | Some(NixSignal::SIGTTOU)
	);
	let wants_grant =
		command.options.grant_foreground && command.options.grouped && !command.options.session;

	let child_pgid = match command_state {
		CommandState::Running { child, .. } => child.id().map(|id| Pid::from_raw(id as i32)),
		_ => None,
	};

	if tty_denied {
		// the kernel paused the command because it touched the terminal while its process
		// group was not the terminal's foreground process group
		if wants_grant {
			if let Some(pgrp) = child_pgid {
				if foreground_grant.is_none() {
					match ForegroundGrant::acquire(pgrp) {
						Ok(grant) => {
							foreground_grant.replace(grant);
							debug!(%pgrp, "granted the terminal foreground to the command");
						}
						Err(error) => {
							debug!(%error, "could not grant the terminal foreground");
						}
					}
				}

				// continue the paused group; this also covers a re-pause while the grant is held
				if let Err(error) = nix::sys::signal::killpg(pgrp, NixSignal::SIGCONT) {
					trace!(%error, "could not continue the paused command");
				}
			} else {
				trace!("command was paused by the terminal but it has already ended");
			}
		} else {
			debug!(signal = ?nix_signal, "command was paused by the terminal");
		}
		return;
	}

	// a deliberate suspension (SIGSTOP/SIGTSTP) or other stop: give the terminal back to
	// the supervisor and restore the terminal state, but leave the command paused.
	if let Some(grant) = foreground_grant.take() {
		trace!("reclaiming the terminal foreground after a suspension");
		grant.release();
	}
	debug!(signal = ?nix_signal, "command was suspended");
}

/// Start the terminal pause watcher for a freshly spawned child, replacing any existing one.
#[cfg(all(unix, not(test)))]
fn start_pause_watch_if_running(
	command_state: &CommandState,
	command: &Command,
	pause_watch: &mut Option<Flag>,
	tx: tokio::sync::mpsc::UnboundedSender<i32>,
) {
	if !command_state.is_running() {
		return;
	}

	if let CommandState::Running { child, .. } = command_state {
		if let Some(pid) = child.id() {
			end_pause_watch(pause_watch);
			*pause_watch = start_pause_watch(pid, &command.options, tx);
		}
	}
}

/// Start a thread which watches a running child for terminal pauses, or `None` if the command
/// options do not ask for it or the thread could not be started.
#[cfg(unix)]
fn start_pause_watch(
	pid: u32,
	options: &crate::command::SpawnOptions,
	tx: tokio::sync::mpsc::UnboundedSender<i32>,
) -> Option<Flag> {
	if !options.grant_foreground {
		return None;
	}

	let cancel = Flag::default();
	match std::thread::Builder::new()
		.name(format!("wx-pause-watch-{pid}"))
		.spawn({
			let cancel = cancel.clone();
			move || pause_watch_loop(pid as i32, cancel, tx)
		}) {
		Ok(_handle) => Some(cancel),
		Err(error) => {
			trace!(%error, "could not start the terminal pause watcher");
			None
		}
	}
}

/// Raise the current pause watcher's cancel flag, if any.
#[cfg(unix)]
fn end_pause_watch(pause_watch: &mut Option<Flag>) {
	if let Some(cancel) = pause_watch.take() {
		cancel.raise();
	}
}

/// Wait for a child status without reaping it.
///
/// Nix does not currently expose `waitid` on Apple targets, even though Darwin provides it.
#[cfg(all(unix, target_vendor = "apple"))]
fn waitid_child(
	pid: Pid,
	flags: nix::sys::wait::WaitPidFlag,
) -> nix::Result<nix::sys::wait::WaitStatus> {
	use nix::{
		errno::Errno,
		libc,
		sys::{signal::Signal, wait::WaitStatus},
	};

	// SAFETY: `siginfo` is zero-initialised because waitid leaves it untouched when WNOHANG
	// finds no matching state change. P_PID selects exactly the child represented by `pid`.
	let siginfo = unsafe {
		let mut siginfo: libc::siginfo_t = std::mem::zeroed();
		Errno::result(libc::waitid(
			libc::P_PID,
			pid.as_raw() as libc::id_t,
			&raw mut siginfo,
			flags.bits(),
		))?;
		siginfo
	};

	// SAFETY: waitid returned a SIGCHLD siginfo value, for which si_pid and si_status are valid.
	let status = unsafe {
		if siginfo.si_pid() == 0 {
			return Ok(WaitStatus::StillAlive);
		}

		match siginfo.si_code {
			libc::CLD_STOPPED => WaitStatus::Stopped(pid, Signal::try_from(siginfo.si_status())?),
			libc::CLD_CONTINUED => WaitStatus::Continued(pid),
			_ => WaitStatus::StillAlive,
		}
	};

	Ok(status)
}

#[cfg(all(unix, not(target_vendor = "apple")))]
fn waitid_child(
	pid: Pid,
	flags: nix::sys::wait::WaitPidFlag,
) -> nix::Result<nix::sys::wait::WaitStatus> {
	use nix::sys::wait::{waitid, Id};

	waitid(Id::Pid(pid), flags)
}

/// Watch a running child for terminal pauses and send the pausing signal numbers over `tx`.
///
/// The loop uses `waitid` with WSTOPPED only, consuming pause events as they come: exit events
/// are never touched, so they remain available for tokio's reaping. It exits when the child is
/// reaped (ECHILD), lost, or the cancel flag is raised.
#[cfg(unix)]
fn pause_watch_loop(pid: i32, cancel: Flag, tx: tokio::sync::mpsc::UnboundedSender<i32>) {
	use nix::errno::Errno;
	use nix::sys::wait::{WaitPidFlag, WaitStatus};

	let pid = Pid::from_raw(pid);
	loop {
		if cancel.raised() {
			return;
		}

		match waitid_child(
			pid,
			WaitPidFlag::WSTOPPED | WaitPidFlag::WNOWAIT | WaitPidFlag::WNOHANG,
		) {
			Ok(WaitStatus::Stopped(..)) => {
				// consume the pause event so the next peek sees the next one; WSTOPPED alone
				// never reports exit events, which remain queued for tokio's reaping
				match waitid_child(pid, WaitPidFlag::WSTOPPED | WaitPidFlag::WNOHANG) {
					Ok(WaitStatus::Stopped(_, signal)) => {
						if tx.send(signal as i32).is_err() {
							return; // the job task is gone
						}
					}
					Ok(_) => {}
					Err(Errno::ECHILD) => return,
					Err(error) => {
						trace!(%error, "pause watcher lost the child");
						return;
					}
				}
			}
			Ok(_) => {}
			Err(Errno::ECHILD) => return, // the child was reaped: the run is over
			Err(error) => {
				trace!(%error, "pause watcher lost the child");
				return;
			}
		}

		std::thread::sleep(Duration::from_millis(50));
	}
}

#[cfg_attr(not(windows), allow(clippy::needless_pass_by_ref_mut))] // needed for start_kill()
#[instrument(level = "trace")]
async fn signal_child(
	signal: Signal,
	#[cfg(not(test))] child: &mut Box<dyn process_wrap::tokio::ChildWrapper>,
	#[cfg(test)] child: &mut super::TestChild,
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
