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

pub enum Control {
	Start,
	Stop,
	GracefulStop { signal: Signal, grace: Duration },
	TryRestart,
	TryGracefulRestart { signal: Signal, grace: Duration },
	Signal(Signal),
	Delete,

	NextEnding,

	SyncFunc(SyncFunc),
	AsyncFunc(AsyncFunc),

	SetSyncSpawnHook(SyncSpawnHook),
	SetAsyncSpawnHook(AsyncSpawnHook),
	UnsetSpawnHook,
	SetSyncErrorHandler(SyncErrorHandler),
	SetAsyncErrorHandler(AsyncErrorHandler),
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
pub(crate) struct ControlMessage {
	pub control: Control,
	pub done: Flag,
}

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
