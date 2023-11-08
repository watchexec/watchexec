use std::{
	fmt::{self, Display},
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
	time::Duration,
};

use command_group::Signal;

use crate::command::Command;

use super::{
	control::{Control, ControlTicket},
	ids::{JobId, TicketId},
	priority::Priority,
};

/// A handle to a command in the supervisor.
///
/// This is the only way to interact with a command.
///
/// All the async methods queue _control tickets_ to the command. Those are executed in order,
/// except where noted. Queueing a control ticket returns a [`TicketId`], which can be used to check
/// the control's status. Note that controls are not guaranteed to run, like if the command task
/// stops or panics before the control is processed.
#[derive(Debug, Clone)]
pub struct Job {
	id: JobId,
	command: Arc<Command>,
	control_queue: async_priority_channel::Sender<ControlTicket, Priority>,

	/// Set to true when the command task terminates.
	gone: Arc<AtomicBool>,
}

impl Job {
	/// The job's ID.
	///
	/// This is unique across all jobs past and present for this supervisor.
	pub fn id(&self) -> JobId {
		self.id
	}

	/// Send a control message to the command.
	///
	/// All control messages are queued in the order they're sent and processed in order.
	pub async fn control(&self, control: Control) -> Result<TicketId, SendError> {
		let ticket = ControlTicket::from(control);
		let ControlTicket { id, .. } = ticket;

		if !self.gone.load(Ordering::Relaxed) {
			self.control_queue.send(ticket, Priority::Normal).await?;
		}

		Ok(id)
	}

	/// Send a signal to the command.
	pub async fn signal(&self, sig: Signal) -> Result<TicketId, SendError> {
		self.control(Control::Signal(sig)).await
	}

	/// Wait for the command to exit, with an optional timeout.
	pub async fn wait(&self, timeout: Option<Duration>) -> Result<TicketId, SendError> {
		self.control(Control::Wait(timeout)).await
	}

	/// Stop the command if it's running.
	///
	/// If `grace > Duration::ZERO`, the command will be sent `signal` and then given `grace` time
	/// before being forcefully terminated.
	pub async fn stop(&self, signal: Signal, grace: Duration) -> Result<TicketId, SendError> {
		self.control(Control::Stop { signal, grace }).await
	}

	/// Start the command if it's not running.
	pub async fn start(&self) -> Result<TicketId, SendError> {
		self.control(Control::Start).await
	}

	/// Start the command, using the given pre-spawn hook.
	pub async fn start_with_hook(
		&self,
		fun: impl FnOnce() + Send + 'static,
	) -> Result<TicketId, SendError> {
		self.control(Control::StartWithHook(Box::new(fun))).await
	}

	/// Restart the command if it's running, or start it if it's not.
	pub async fn restart(&self, signal: Signal, grace: Duration) -> Result<TicketId, SendError> {
		let stop = Control::Stop { signal, grace }.into();
		let start = Control::Start.into();
		let ControlTicket { id, .. } = stop;

		if !self.gone.load(Ordering::Relaxed) {
			self.control_queue
				.sendv(
					[(stop, Priority::Normal), (start, Priority::Normal)]
						.into_iter()
						.peekable(),
				)
				.await?;
		}

		Ok(id)
	}

	/// Restart the command if it's running, but don't start it if it's not.
	pub async fn try_restart(
		&self,
		signal: Signal,
		grace: Duration,
	) -> Result<TicketId, SendError> {
		self.control(Control::TryRestart { signal, grace }).await
	}

	/// Run a hook.
	pub async fn run_hook(
		&self,
		fun: impl FnOnce() + Send + 'static,
	) -> Result<TicketId, SendError> {
		self.control(Control::Hook(Box::new(fun))).await
	}

	async fn delete_with(
		&self,
		priority: Priority,
		signal: Signal,
		grace: Duration,
	) -> Result<TicketId, SendError> {
		let stop = Control::Stop { signal, grace }.into();
		let delete = Control::Delete(self.gone.clone()).into();
		let ControlTicket { id, .. } = stop;

		if !self.gone.load(Ordering::Relaxed) {
			self.control_queue
				.sendv(
					[(stop, priority), (delete, priority)]
						.into_iter()
						.peekable(),
				)
				.await?;
		}

		Ok(id)
	}

	/// Stop the command, then mark it for garbage collection.
	///
	/// The underlying control messages are sent like normal, so they wait for all pending controls
	/// to process. If you want to delete the command immediately, use `delete_now()`.
	///
	/// The arguments are the same as for `stop()`.
	pub async fn delete(&self, signal: Signal, grace: Duration) -> Result<TicketId, SendError> {
		self.delete_with(Priority::Normal, signal, grace).await
	}

	/// Stop the command immediately, then mark it for garbage collection.
	///
	/// The underlying control messages are sent with higher priority than normal, so they bypass
	/// all others. If you want to delete after all current controls are processed, use `delete()`.
	///
	/// The arguments are the same as for `stop()`.
	pub async fn delete_now(&self, signal: Signal, grace: Duration) -> Result<TicketId, SendError> {
		self.delete_with(Priority::Urgent, signal, grace).await
	}
}

impl PartialEq for Job {
	fn eq(&self, other: &Self) -> bool {
		self.id == other.id
	}
}

/// Error when sending a control message.
///
/// This can only happen if the command task panics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendError;

impl<T> From<async_priority_channel::SendError<T>> for SendError {
	fn from(_: async_priority_channel::SendError<T>) -> Self {
		Self
	}
}

impl Display for SendError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "failed to send control message")
	}
}
