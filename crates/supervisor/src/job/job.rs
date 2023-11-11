use std::{
	fmt::{self, Display},
	future::Future,
	sync::Arc,
	time::Duration,
};

use command_group::Signal;
use tokio::process::Command as TokioCommand;

use crate::{
	command::{Command, Program},
	flag::Flag,
};

use super::{
	messages::{Control, ControlMessage, Ticket},
	priority::Priority,
	task::SyncIoError,
	StateSequence,
};

/// A handle to a job task spawned in the supervisor.
///
/// A job is a task which manages a [`Command`]. It is responsible for spawning the command and
/// handling control messages sent to it. It also manages the command's lifetime, and will collect
/// its exit status.
///
/// All the async methods here queue [`Control`]s to the job task and return [`Ticket`]s. Controls
/// execute in order, except where noted. Tickets are futures which resolve when the corresponding
/// control has been run. Unlike most futures, tickets don't need to be polled for controls to make
/// progress; the future is only used to signal completion. Dropping a ticket will not drop the
/// control, so it's safe to do so if you don't care about when the control completes.
///
/// Note that controls are not guaranteed to run, like if the job task stops or panics before a
/// control is processed. If a job task stops gracefully, all pending tickets will resolve
/// immediately. If a job task panics (outside of hooks, panics are bugs!), pending tickets will
/// never resolve.
#[derive(Debug, Clone)]
pub struct Job {
	pub(crate) command: Arc<Command>,
	pub(crate) control_queue: async_priority_channel::Sender<ControlMessage, Priority>,

	/// Set to true when the command task has stopped gracefully.
	pub(crate) gone: Flag,
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
	/// If `grace > Duration::ZERO`, the current program will be sent `signal` and then given
	/// `grace` time before being forcefully terminated. If the current program is in the middle of
	/// the command sequence, the next program is not started; use `skip` if you want to do that.
	pub async fn stop(&self, signal: Signal, grace: Duration) -> Result<Ticket, SendError> {
		self.control(Control::Stop { signal, grace }).await
	}

	/// Start the command if it's not running.
	pub async fn start(&self) -> Result<Ticket, SendError> {
		self.control(Control::Start).await
	}

	/// Skip to the next program in the sequence.
	///
	/// Stops the currently running program, if any, and starts the next one in the sequence if
	/// there is one. Takes the same arguments as [`Job::stop`] for an optional graceful stop.
	pub async fn skip(&self, signal: Signal, grace: Duration) -> Result<Ticket, SendError> {
		self.control(Control::Skip { signal, grace }).await
	}

	/// Set the spawn hook.
	///
	/// The hook will be called once per process spawned, before the process is spawned. It's given
	/// a mutable reference to the [`tokio::process::Command`] and some context; it can modify the
	/// command as it sees fit.
	pub async fn set_spawn_hook(
		&self,
		fun: impl Fn(&mut TokioCommand, &Program) + Send + Sync + 'static,
	) -> Result<Ticket, SendError> {
		self.control(Control::SetSyncSpawnHook(Arc::new(fun))).await
	}

	/// Set the spawn hook (async version).
	///
	/// The hook will be called once per process spawned, before the process is spawned. It's given
	/// a mutable reference to the [`tokio::process::Command`] and some context; it can modify the
	/// command as it sees fit.
	pub async fn set_spawn_async_hook(
		&self,
		fun: impl (Fn(&mut TokioCommand, &Program) -> Box<dyn Future<Output = ()> + Send + Sync>)
			+ Send
			+ Sync
			+ 'static,
	) -> Result<Ticket, SendError> {
		self.control(Control::SetAsyncSpawnHook(Arc::new(fun)))
			.await
	}

	/// Unset any spawn hook.
	pub async fn unset_spawn_hook(&self) -> Result<Ticket, SendError> {
		self.control(Control::UnsetSpawnHook).await
	}

	/// Set the error handler.
	pub async fn set_error_handler(
		&self,
		fun: impl Fn(SyncIoError) + Send + Sync + 'static,
	) -> Result<Ticket, SendError> {
		self.control(Control::SetSyncErrorHandler(Arc::new(fun)))
			.await
	}

	/// Set the error handler (async version).
	pub async fn set_async_error_handler(
		&self,
		fun: impl (Fn(SyncIoError) -> Box<dyn Future<Output = ()> + Send + Sync>)
			+ Send
			+ Sync
			+ 'static,
	) -> Result<Ticket, SendError> {
		self.control(Control::SetAsyncErrorHandler(Arc::new(fun)))
			.await
	}

	/// Unset the error handler.
	///
	/// Errors will be silently ignored.
	pub async fn unset_error_handler(&self) -> Result<Ticket, SendError> {
		self.control(Control::UnsetErrorHandler).await
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

	/// Run an arbitrary function.
	///
	/// The function is given [`&StateSequence`](StateSequence): the state of the command sequence,
	/// including the currently running program, and the exit status of past ones, plus timings.
	///
	/// Technically, some operations can be done through a `&self` shared borrow on the running
	/// program [`ErasedChild`](command_group::tokio::ErasedChild), but this library recommends
	/// against taking advantage of this, and prefer using the methods here instead, so that the
	/// supervisor can keep track of what's going on.
	pub async fn run(
		&self,
		fun: impl FnOnce(&StateSequence) + Send + Sync + 'static,
	) -> Result<Ticket, SendError> {
		self.control(Control::SyncFunc(Box::new(fun))).await
	}

	/// Run an arbitrary function and await the returned future.
	///
	/// The function is given [`&StateSequence`](StateSequence): the state of the command sequence,
	/// including the currently running program, and the exit status of past ones, plus timings.
	///
	/// Technically, some operations can be done through a `&self` shared borrow on the running
	/// program [`ErasedChild`](command_group::tokio::ErasedChild), but this library recommends
	/// against taking advantage of this, and prefer using the methods here instead, so that the
	/// supervisor can keep track of what's going on.
	pub async fn run_async(
		&self,
		fun: impl (FnOnce(&StateSequence) -> Box<dyn Future<Output = ()> + Send + Sync>)
			+ Send
			+ Sync
			+ 'static,
	) -> Result<Ticket, SendError> {
		self.control(Control::AsyncFunc(Box::new(fun))).await
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
			let (ticket, delete) = self.prepare_control(Control::Delete);
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
