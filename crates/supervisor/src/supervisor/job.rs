use std::{
	fmt::{self, Display},
	sync::Arc,
	time::Duration,
};

use command_group::Signal;

use crate::{command::Command, flag::Flag};

use super::{
	control::{Control, ControlMessage, Ticket},
	priority::Priority,
};

/// A handle to a command task spawned in the supervisor.
///
/// All the async methods queue [`Control`]s to the command and return [`Ticket`]s. Controls are
/// executed in order, except where noted. Tickets are futures which resolve when the corresponding
/// control has been run. Unlike most futures, tickets don't need to be polled for controls to make
/// progress; the future is only used to signal completion. Dropping a ticket will not drop the
/// control, so it's safe to do so if you don't care about when the control completes.
///
/// Note that controls are not guaranteed to run, like if the command task stops or panics before
/// a control is processed. If a command task stops gracefully, all pending tickets will resolve
/// immediately. If a command task panics, pending tickets will never resolve.
#[derive(Debug, Clone)]
pub struct Job {
	command: Arc<Command>,
	control_queue: async_priority_channel::Sender<ControlMessage, Priority>,

	/// Set to true when the command task has stopped gracefully.
	gone: Flag,
}

impl Job {
	/// The [`Command`] this job is managing.
	pub fn command(&self) -> Arc<Command> {
		self.command.clone()
	}

	fn prepare_control(&self, control: Control) -> (Ticket, ControlMessage) {
		let done = Flag::default();
		(
			Ticket {
				job_gone: self.gone.clone(),
				control_done: done.clone(),
			},
			ControlMessage { control, done },
		)
	}

	/// Send a control message to the command.
	///
	/// All control messages are queued in the order they're sent and processed in order.
	pub async fn control(&self, control: Control) -> Result<Ticket, SendError> {
		if self.gone.raised() {
			Ok(Ticket::cancelled())
		} else {
			let (ticket, control) = self.prepare_control(control);
			self.control_queue.send(control, Priority::Normal).await?;
			Ok(ticket)
		}
	}

	/// Send a signal to the command.
	pub async fn signal(&self, sig: Signal) -> Result<Ticket, SendError> {
		self.control(Control::Signal(sig)).await
	}

	/// Wait for the command to exit, with an optional timeout.
	pub async fn wait(&self, timeout: Option<Duration>) -> Result<Ticket, SendError> {
		self.control(Control::Wait(timeout)).await
	}

	/// Stop the command if it's running.
	///
	/// If `grace > Duration::ZERO`, the command will be sent `signal` and then given `grace` time
	/// before being forcefully terminated.
	pub async fn stop(&self, signal: Signal, grace: Duration) -> Result<Ticket, SendError> {
		self.control(Control::Stop { signal, grace }).await
	}

	/// Start the command if it's not running.
	pub async fn start(&self) -> Result<Ticket, SendError> {
		self.control(Control::Start).await
	}

	/// Start the command, using the given pre-spawn hook.
	pub async fn start_with_hook(
		&self,
		fun: impl FnOnce() + Send + 'static,
	) -> Result<Ticket, SendError> {
		self.control(Control::StartWithHook(Box::new(fun))).await
	}

	/// Restart the command if it's running, or start it if it's not.
	pub async fn restart(&self, signal: Signal, grace: Duration) -> Result<Ticket, SendError> {
		if self.gone.raised() {
			Ok(Ticket::cancelled())
		} else {
			let (_, stop) = self.prepare_control(Control::Stop { signal, grace });
			let (ticket, start) = self.prepare_control(Control::Start);
			self.control_queue
				.sendv(
					[(stop, Priority::Normal), (start, Priority::Normal)]
						.into_iter()
						.peekable(),
				)
				.await?;
			Ok(ticket)
		}
	}

	/// Restart the command if it's running, but don't start it if it's not.
	pub async fn try_restart(&self, signal: Signal, grace: Duration) -> Result<Ticket, SendError> {
		self.control(Control::TryRestart { signal, grace }).await
	}

	/// Run a hook.
	pub async fn run_hook(&self, fun: impl FnOnce() + Send + 'static) -> Result<Ticket, SendError> {
		self.control(Control::Hook(Box::new(fun))).await
	}

	async fn delete_with(
		&self,
		priority: Priority,
		signal: Signal,
		grace: Duration,
	) -> Result<Ticket, SendError> {
		if self.gone.raised() {
			Ok(Ticket::cancelled())
		} else {
			let (_, stop) = self.prepare_control(Control::Stop { signal, grace });
			let (ticket, delete) = self.prepare_control(Control::Delete(self.gone.clone()));
			self.control_queue
				.sendv(
					[(stop, priority), (delete, priority)]
						.into_iter()
						.peekable(),
				)
				.await?;
			Ok(ticket)
		}
	}

	/// Stop the command, then mark it for garbage collection.
	///
	/// The underlying control messages are sent like normal, so they wait for all pending controls
	/// to process. If you want to delete the command immediately, use `delete_now()`.
	///
	/// The arguments are the same as for `stop()`.
	pub async fn delete(&self, signal: Signal, grace: Duration) -> Result<Ticket, SendError> {
		self.delete_with(Priority::Normal, signal, grace).await
	}

	/// Stop the command immediately, then mark it for garbage collection.
	///
	/// The underlying control messages are sent with higher priority than normal, so they bypass
	/// all others. If you want to delete after all current controls are processed, use `delete()`.
	///
	/// The arguments are the same as for `stop()`.
	pub async fn delete_now(&self, signal: Signal, grace: Duration) -> Result<Ticket, SendError> {
		self.delete_with(Priority::Urgent, signal, grace).await
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
