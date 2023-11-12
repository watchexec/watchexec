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
	errors::SyncIoError,
	flag::Flag,
};

use super::{
	messages::{Control, ControlMessage, Ticket},
	priority::Priority,
	JobTaskContext,
};

/// A handle to a job task spawned in the supervisor.
///
/// A job is a task which manages a [`Command`]. It is responsible for spawning the programs in the
/// order determined by the command's [`Sequence`](crate::command::Sequence), and for handling
/// messages which control it, for managing the programs' lifetimes, and for collecting their exit
/// statuses and some timing information.
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

	async fn send_controls(
		&self,
		mut controls: Vec<Control>,
		priority: Priority,
	) -> Result<Ticket, SendError> {
		if self.gone.raised() {
			Ok(Ticket::cancelled())
		} else if controls.len() == 1 {
			let (ticket, control) = self.prepare_control(controls.pop().unwrap());
			self.control_queue.send(control, priority).await?;
			Ok(ticket)
		} else {
			let (mut tickets, controls): (Vec<Ticket>, Vec<ControlMessage>) = controls
				.into_iter()
				.map(|control| self.prepare_control(control))
				.unzip();
			let ticket = tickets.pop().expect("controls should always be non-empty");

			self.control_queue
				.sendv(
					controls
						.into_iter()
						.map(|control| (control, priority))
						.peekable(),
				)
				.await?;
			Ok(ticket)
		}
	}

	/// Send a control message to the command.
	///
	/// All control messages are queued in the order they're sent and processed in order.
	pub async fn control(&self, control: Control) -> Result<Ticket, SendError> {
		self.send_controls(vec![control], Priority::Normal).await
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

	/// Stop the command if it's running.
	///
	/// If `grace > Duration::ZERO`, the current program will be sent `signal` and then given
	/// `grace` time before being forcefully terminated. If the current program is in the middle of
	/// the command sequence, the next program is not started; use `skip` if you want to do that.
	pub async fn stop(&self, signal: Signal, grace: Duration) -> Result<Ticket, SendError> {
		self.control(Control::Stop { signal, grace }).await
	}

	/// Restart the command if it's running, or start it if it's not.
	///
	/// Stops the currently running program, if any, using the same logic as [`Job::stop`]; the
	/// `signal` and `grace` arguments are used for an optional graceful stop.
	pub async fn restart(&self, signal: Signal, grace: Duration) -> Result<Ticket, SendError> {
		self.send_controls(
			vec![Control::Stop { signal, grace }, Control::Start],
			Priority::Normal,
		)
		.await
	}

	/// Restart the command if it's running, but don't start it if it's not.
	///
	/// Stops the currently running program, if any, using the same logic as [`Job::stop`]; the
	/// `signal` and `grace` arguments are used for an optional graceful stop.
	pub async fn try_restart(&self, signal: Signal, grace: Duration) -> Result<Ticket, SendError> {
		self.control(Control::TryRestart { signal, grace }).await
	}

	/// Send a signal to the command.
	///
	/// Sends a signal to the current program, if there is one. If there isn't, this is a no-op.
	pub async fn signal(&self, sig: Signal) -> Result<Ticket, SendError> {
		self.control(Control::Signal(sig)).await
	}

	async fn delete_with(
		&self,
		priority: Priority,
		signal: Signal,
		grace: Duration,
	) -> Result<Ticket, SendError> {
		self.send_controls(
			vec![Control::Stop { signal, grace }, Control::Delete],
			priority,
		)
		.await
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

	/// Get a future which resolves when the current program ends.
	///
	/// If the command sequence is not running, the future resolves immediately.
	///
	/// The underlying control message is sent with higher priority than normal, so it targets the
	/// actively running program, not the one that will be running after the rest of the controls
	/// get done; note that may still be racy if the program ends between the time the message is
	/// sent and the time it's processed.
	pub async fn until_program_end(&self) -> Result<Ticket, SendError> {
		self.send_controls(vec![Control::NextEnding], Priority::High)
			.await
	}

	/// Get a future which resolves when the current command sequence ends.
	///
	/// If the command sequence is not running, the future resolves immediately.
	///
	/// The underlying control message is sent with higher priority than normal, so it targets the
	/// actively running sequence, not the one that will be running after the rest of the controls
	/// get done; note that may still be racy if the sequence ends between the time the message is
	/// sent and the time it's processed.
	pub async fn until_sequence_end(&self) -> Result<Ticket, SendError> {
		self.send_controls(vec![Control::SequenceEnding], Priority::High)
			.await
	}

	/// Run an arbitrary function.
	///
	/// The function is given [`&JobTaskContext`](JobTaskContext), which contains the state of the
	/// currently executing, next-to-start, or just-finished command sequence, as well as the final
	/// state of the _last_ run of the sequence.
	///
	/// Technically, some operations can be done through a `&self` shared borrow on the running
	/// program [`ErasedChild`](command_group::tokio::ErasedChild), but this library recommends
	/// against taking advantage of this, and prefer using the methods here instead, so that the
	/// supervisor can keep track of what's going on.
	pub async fn run(
		&self,
		fun: impl FnOnce(&JobTaskContext) + Send + Sync + 'static,
	) -> Result<Ticket, SendError> {
		self.control(Control::SyncFunc(Box::new(fun))).await
	}

	/// Run an arbitrary function and await the returned future.
	///
	/// The function is given [`&JobTaskContext`](JobTaskContext), which contains the state of the
	/// currently executing, next-to-start, or just-finished command sequence, as well as the final
	/// state of the _last_ run of the sequence.
	///
	/// Technically, some operations can be done through a `&self` shared borrow on the running
	/// program [`ErasedChild`](command_group::tokio::ErasedChild), but this library recommends
	/// against taking advantage of this, and prefer using the methods here instead, so that the
	/// supervisor can keep track of what's going on.
	pub async fn run_async(
		&self,
		fun: impl (FnOnce(&JobTaskContext) -> Box<dyn Future<Output = ()> + Send + Sync>)
			+ Send
			+ Sync
			+ 'static,
	) -> Result<Ticket, SendError> {
		self.control(Control::AsyncFunc(Box::new(fun))).await
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
