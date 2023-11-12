use std::{
	future::Future,
	pin::Pin,
	task::{Context, Poll},
	time::Duration,
};

use command_group::Signal;
use futures::{future::select, FutureExt};

use crate::flag::Flag;

use super::task::{
	AsyncErrorHandler, AsyncFunc, AsyncSpawnHook, SyncErrorHandler, SyncFunc, SyncSpawnHook,
};

pub enum Control {
	Start,
	Skip { signal: Signal, grace: Duration },
	Stop { signal: Signal, grace: Duration },
	TryRestart { signal: Signal, grace: Duration },
	Wait(Option<Duration>),
	Signal(Signal),
	Delete,

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
			Self::Skip { signal, grace } => f
				.debug_struct("Skip")
				.field("signal", signal)
				.field("grace", grace)
				.finish(),
			Self::Stop { signal, grace } => f
				.debug_struct("Stop")
				.field("signal", signal)
				.field("grace", grace)
				.finish(),
			Self::TryRestart { signal, grace } => f
				.debug_struct("TryRestart")
				.field("signal", signal)
				.field("grace", grace)
				.finish(),
			Self::Wait(timeout) => f.debug_struct("Wait").field("timeout", timeout).finish(),
			Self::Signal(signal) => f.debug_struct("Signal").field("signal", signal).finish(),
			Self::Delete => f.debug_struct("Delete").finish(),

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
