use std::time::Duration;

use tokio::{
	select,
	sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
	time::{sleep_until, Instant, Sleep},
};

use crate::flag::Flag;

use super::{messages::ControlMessage, Control};

#[derive(Debug, Copy, Clone)]
pub enum Priority {
	Normal,
	High,
	Urgent,
}

#[derive(Debug)]
pub struct PriorityReceiver {
	pub normal: UnboundedReceiver<ControlMessage>,
	pub high: UnboundedReceiver<ControlMessage>,
	pub urgent: UnboundedReceiver<ControlMessage>,
}

#[derive(Debug, Clone)]
pub struct PrioritySender {
	pub normal: UnboundedSender<ControlMessage>,
	pub high: UnboundedSender<ControlMessage>,
	pub urgent: UnboundedSender<ControlMessage>,
}

impl PrioritySender {
	pub fn send(&self, message: ControlMessage, priority: Priority) {
		// drop errors: if the channel is closed, the job is dead
		let _ = match priority {
			Priority::Normal => self.normal.send(message),
			Priority::High => self.high.send(message),
			Priority::Urgent => self.urgent.send(message),
		};
	}
}

impl PriorityReceiver {
	/// Receive a control message from the command.
	///
	/// If `stop_timer` is `Some`, normal priority messages are not received; instead, only high and
	/// urgent priority messages are received until the timer expires, and when the timer completes,
	/// a `Stop` control message is returned and the `stop_timer` is `None`d.
	///
	/// This is used to implement stop's, restart's, and try-restart's graceful stopping logic.
	pub async fn recv(&mut self, stop_timer: &mut Option<Timer>) -> Option<ControlMessage> {
		if stop_timer.as_ref().map_or(false, Timer::is_past) {
			return stop_timer.take().map(|timer| timer.to_control());
		}

		if let Ok(message) = self.urgent.try_recv() {
			return Some(message);
		}

		if let Ok(message) = self.high.try_recv() {
			return Some(message);
		}

		if let Some(timer) = stop_timer.clone() {
			select! {
				_ = timer.to_sleep() => {
					*stop_timer = None;
					Some(timer.to_control())
				}
				message = self.urgent.recv() => message,
				message = self.high.recv() => message,
			}
		} else {
			select! {
				message = self.urgent.recv() => message,
				message = self.high.recv() => message,
				message = self.normal.recv() => message,
			}
		}
	}
}

pub fn new() -> (PrioritySender, PriorityReceiver) {
	let (normal_tx, normal_rx) = unbounded_channel();
	let (high_tx, high_rx) = unbounded_channel();
	let (urgent_tx, urgent_rx) = unbounded_channel();

	(
		PrioritySender {
			normal: normal_tx,
			high: high_tx,
			urgent: urgent_tx,
		},
		PriorityReceiver {
			normal: normal_rx,
			high: high_rx,
			urgent: urgent_rx,
		},
	)
}

#[derive(Debug, Clone)]
pub struct Timer {
	pub until: Instant,
	pub done: Flag,
	pub is_restart: bool,
}

impl Timer {
	pub fn stop(grace: Duration, done: Flag) -> Self {
		Self {
			until: Instant::now() + grace,
			done,
			is_restart: false,
		}
	}

	pub fn restart(grace: Duration, done: Flag) -> Self {
		Self {
			until: Instant::now() + grace,
			done,
			is_restart: true,
		}
	}

	fn to_sleep(&self) -> Sleep {
		sleep_until(self.until)
	}

	fn is_past(&self) -> bool {
		self.until <= Instant::now()
	}

	fn to_control(&self) -> ControlMessage {
		ControlMessage {
			control: if self.is_restart {
				Control::ContinueTryGracefulRestart
			} else {
				Control::Stop
			},
			done: self.done.clone(),
		}
	}
}
