use std::{
	future::Future,
	pin::Pin,
	task::{Context, Poll},
	time::Duration,
};

use futures::{future::select, FutureExt};
use watchexec_signals::Signal;

use crate::flag::Flag;

use super::task::{
	AsyncErrorHandler, AsyncFunc, AsyncSpawnHook, SyncErrorHandler, SyncFunc, SyncSpawnHook,
};

/// The underlying control message types for [`Job`](super::Job).
///
/// You may use [`Job::control()`](super::Job::control()) to send these messages directly, but in
/// general should prefer the higher-level methods on [`Job`](super::Job) itself.
pub enum Control {
	/// For [`Job::start()`](super::Job::start()).
	Start,
	/// For [`Job::stop()`](super::Job::stop()).
	Stop,
	/// For [`Job::stop_with_signal()`](super::Job::stop_with_signal()).
	GracefulStop {
		/// Signal to send immediately
		signal: Signal,
		/// Time to wait before forceful termination
		grace: Duration,
	},
	/// For [`Job::try_restart()`](super::Job::try_restart()).
	TryRestart,
	/// For [`Job::try_restart_with_signal()`](super::Job::try_restart_with_signal()).
	TryGracefulRestart {
		/// Signal to send immediately
		signal: Signal,
		/// Time to wait before forceful termination and restart
		grace: Duration,
	},
	/// Internal implementation detail of [`Control::TryGracefulRestart`].
	ContinueTryGracefulRestart,
	/// For [`Job::signal()`](super::Job::signal()).
	Signal(Signal),
	/// For [`Job::delete()`](super::Job::delete()) and [`Job::delete_now()`](super::Job::delete_now()).
	Delete,

	/// For [`Job::to_wait()`](super::Job::to_wait()).
	NextEnding,

	/// For [`Job::run()`](super::Job::run()).
	SyncFunc(SyncFunc),
	/// For [`Job::run_async()`](super::Job::run_async()).
	AsyncFunc(AsyncFunc),

	/// For [`Job::set_spawn_hook()`](super::Job::set_spawn_hook()).
	SetSyncSpawnHook(SyncSpawnHook),
	/// For [`Job::set_spawn_async_hook()`](super::Job::set_spawn_async_hook()).
	SetAsyncSpawnHook(AsyncSpawnHook),
	/// For [`Job::unset_spawn_hook()`](super::Job::unset_spawn_hook()).
	UnsetSpawnHook,
	/// For [`Job::set_error_handler()`](super::Job::set_error_handler()).
	SetSyncErrorHandler(SyncErrorHandler),
	/// For [`Job::set_async_error_handler()`](super::Job::set_async_error_handler()).
	SetAsyncErrorHandler(AsyncErrorHandler),
	/// For [`Job::unset_error_handler()`](super::Job::unset_error_handler()).
	UnsetErrorHandler,
}

impl std::fmt::Debug for Control {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Start => f.debug_struct("Start").finish(),
			Self::Stop => f.debug_struct("Stop").finish(),
			Self::GracefulStop { signal, grace } => f
				.debug_struct("GracefulStop")
				.field("signal", signal)
				.field("grace", grace)
				.finish(),
			Self::TryRestart => f.debug_struct("TryRestart").finish(),
			Self::TryGracefulRestart { signal, grace } => f
				.debug_struct("TryGracefulRestart")
				.field("signal", signal)
				.field("grace", grace)
				.finish(),
			Self::ContinueTryGracefulRestart => {
				f.debug_struct("ContinueTryGracefulRestart").finish()
			}
			Self::Signal(signal) => f.debug_struct("Signal").field("signal", signal).finish(),
			Self::Delete => f.debug_struct("Delete").finish(),

			Self::NextEnding => f.debug_struct("NextEnding").finish(),

			Self::SyncFunc(_) => f.debug_struct("SyncFunc").finish_non_exhaustive(),
			Self::AsyncFunc(_) => f.debug_struct("AsyncFunc").finish_non_exhaustive(),

			Self::SetSyncSpawnHook(_) => f.debug_struct("SetSyncSpawnHook").finish_non_exhaustive(),
			Self::SetAsyncSpawnHook(_) => {
				f.debug_struct("SetSpawnAsyncHook").finish_non_exhaustive()
			}
			Self::UnsetSpawnHook => f.debug_struct("UnsetSpawnHook").finish(),
			Self::SetSyncErrorHandler(_) => f
				.debug_struct("SetSyncErrorHandler")
				.finish_non_exhaustive(),
			Self::SetAsyncErrorHandler(_) => f
				.debug_struct("SetAsyncErrorHandler")
				.finish_non_exhaustive(),
			Self::UnsetErrorHandler => f.debug_struct("UnsetErrorHandler").finish(),
		}
	}
}

#[derive(Debug)]
pub struct ControlMessage {
	pub control: Control,
	pub done: Flag,
}

/// Lightweight future which resolves when the corresponding control has been run.
///
/// Unlike most futures, tickets don't need to be polled for controls to make progress; the future
/// is only used to signal completion. Dropping a ticket will not drop the control, so it's safe to
/// do so if you don't care about when the control completes.
///
/// Tickets can be cloned, and all clones will resolve at the same time.
#[derive(Debug, Clone)]
pub struct Ticket {
	pub(crate) job_gone: Flag,
	pub(crate) control_done: Flag,
}

impl Ticket {
	pub(crate) fn cancelled() -> Self {
		Self {
			job_gone: Flag::new(true),
			control_done: Flag::new(true),
		}
	}
}

impl Future for Ticket {
	type Output = ();

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		Pin::new(&mut select(self.job_gone.clone(), self.control_done.clone()).map(|_| ())).poll(cx)
	}
}
