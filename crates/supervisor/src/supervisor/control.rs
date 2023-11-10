use std::{
	future::Future,
	pin::Pin,
	task::{Context, Poll},
	time::Duration,
};

use command_group::Signal;
use futures::{future::select, FutureExt};
use tokio::process::Command as TokioCommand;

use crate::{command::Program, flag::Flag};

use super::job::Job;

pub enum Control {
	Signal(Signal),
	Wait(Option<Duration>),
	Stop {
		signal: Signal,
		grace: Duration,
	},
	Start,
	StartWithAsyncHook(
		Box<dyn (FnOnce(&mut TokioCommand, &Program) -> dyn Future<Output = ()>) + Send + 'static>,
	),
	StartWithHook(Box<dyn FnOnce(&mut TokioCommand, &Program) + Send + 'static>),
	TryRestart {
		signal: Signal,
		grace: Duration,
	},
	AsyncHook(Box<dyn (FnOnce() -> dyn Future<Output = ()>) + Send + 'static>),
	Hook(Box<dyn FnOnce() + Send + 'static>),
	Delete(Flag),
}

impl std::fmt::Debug for Control {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Signal(signal) => f.debug_struct("Signal").field("signal", signal).finish(),
			Self::Wait(timeout) => f.debug_struct("Wait").field("timeout", timeout).finish(),
			Self::Stop { signal, grace } => f
				.debug_struct("Stop")
				.field("signal", signal)
				.field("grace", grace)
				.finish(),
			Self::Start => write!(f, "Start"),
			Self::StartWithAsyncHook(_) => {
				f.debug_struct("StartWithAsyncHook").finish_non_exhaustive()
			}
			Self::StartWithHook(_) => f.debug_struct("StartWithHook").finish_non_exhaustive(),
			Self::TryRestart { signal, grace } => f
				.debug_struct("TryRestart")
				.field("signal", signal)
				.field("grace", grace)
				.finish(),
			Self::AsyncHook(_) => f.debug_struct("AsyncHook").finish_non_exhaustive(),
			Self::Hook(_) => f.debug_struct("Hook").finish_non_exhaustive(),
			Self::Delete(gone) => f.debug_struct("Delete").field("gone", gone).finish(),
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
