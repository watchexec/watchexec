use std::{time::Duration, sync::{atomic::AtomicBool, Arc}};

use command_group::Signal;

use super::ids::TicketId;

pub enum Control {
	Signal(Signal),
	Wait(Option<Duration>),
	Stop { signal: Signal, grace: Duration },
	Start,
	StartWithHook(Box<dyn FnOnce() + Send + 'static>),
	TryRestart { signal: Signal, grace: Duration },
	Hook(Box<dyn FnOnce() + Send + 'static>),
	Delete(Arc<AtomicBool>),
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
			Self::StartWithHook(arg0) => f.debug_struct("StartWithHook").finish_non_exhaustive(),
			Self::TryRestart { signal, grace } => f
				.debug_struct("TryRestart")
				.field("signal", signal)
				.field("grace", grace)
				.finish(),
			Self::Hook(arg0) => f.debug_struct("Hook").finish_non_exhaustive(),
			Self::Delete(gone) => f.debug_struct("Delete").field("gone", gone).finish(),
		}
	}
}

#[derive(Debug)]
pub struct ControlTicket {
	pub id: TicketId,
	pub control: Control,
}

impl From<Control> for ControlTicket {
	fn from(control: Control) -> Self {
		Self {
			control,
			id: TicketId::default(),
		}
	}
}
