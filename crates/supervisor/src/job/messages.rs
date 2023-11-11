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
	SetAsyncSpawnHook(AsyncSpawnHook),
	SetSyncSpawnHook(SyncSpawnHook),
	UnsetSpawnHook,
	SetAsyncErrorHandler(AsyncErrorHandler),
	SetSyncErrorHandler(SyncErrorHandler),
	UnsetErrorHandler,

	AsyncFunc(AsyncFunc),
	SyncFunc(SyncFunc),

	Signal(Signal),
	Wait(Option<Duration>),
	Skip { signal: Signal, grace: Duration },
	Stop { signal: Signal, grace: Duration },
	Start,
	TryRestart { signal: Signal, grace: Duration },
	Delete,
}

impl std::fmt::Debug for Control {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Signal(signal) => f.debug_struct("Signal").field("signal", signal).finish(),
			Self::Wait(timeout) => f.debug_struct("Wait").field("timeout", timeout).finish(),
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
			Self::Start => f.debug_struct("Start").finish(),
			Self::SetAsyncSpawnHook(_) => {
				f.debug_struct("SetSpawnAsyncHook").finish_non_exhaustive()
			}
			Self::SetSyncSpawnHook(_) => f.debug_struct("SetSyncSpawnHook").finish_non_exhaustive(),
			Self::UnsetSpawnHook => f.debug_struct("UnsetSpawnHook").finish(),
			Self::SetAsyncErrorHandler(_) => f
				.debug_struct("SetAsyncErrorHandler")
				.finish_non_exhaustive(),
			Self::SetSyncErrorHandler(_) => f
				.debug_struct("SetSyncErrorHandler")
				.finish_non_exhaustive(),
			Self::UnsetErrorHandler => f.debug_struct("UnsetErrorHandler").finish(),
			Self::TryRestart { signal, grace } => f
				.debug_struct("TryRestart")
				.field("signal", signal)
				.field("grace", grace)
				.finish(),
			Self::AsyncFunc(_) => f.debug_struct("AsyncFunc").finish_non_exhaustive(),
			Self::SyncFunc(_) => f.debug_struct("SyncFunc").finish_non_exhaustive(),
			Self::Delete => f.debug_struct("Delete").finish(),
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
