use tokio::{
	select,
	sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
};

use super::messages::ControlMessage;

#[derive(Debug, Copy, Clone)]
pub enum Priority {
	Normal,
	High,
	Urgent,
}

#[derive(Debug)]
pub(crate) struct PriorityReceiver {
	pub normal: UnboundedReceiver<ControlMessage>,
	pub high: UnboundedReceiver<ControlMessage>,
	pub urgent: UnboundedReceiver<ControlMessage>,
}

#[derive(Debug, Clone)]
pub(crate) struct PrioritySender {
	pub normal: UnboundedSender<ControlMessage>,
	pub high: UnboundedSender<ControlMessage>,
	pub urgent: UnboundedSender<ControlMessage>,
}

impl PrioritySender {
	pub fn send(&self, message: ControlMessage, priority: Priority) {
		// drop errors: if the channel is closed, the job is dead
		match priority {
			Priority::Normal => self.normal.send(message),
			Priority::High => self.high.send(message),
			Priority::Urgent => self.urgent.send(message),
		};
	}
}

impl PriorityReceiver {
	pub async fn recv(&mut self) -> Option<ControlMessage> {
		if let Ok(message) = self.urgent.try_recv() {
			return Some(message);
		}

		if let Ok(message) = self.high.try_recv() {
			return Some(message);
		}

		select! {
			message = self.urgent.recv() => message,
			message = self.high.recv() => message,
			message = self.normal.recv() => message,
		}
	}
}

pub(crate) fn new() -> (PrioritySender, PriorityReceiver) {
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
