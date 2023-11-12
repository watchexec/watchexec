use std::{future::Future, sync::Arc, time::Duration};

use tokio::process::Command as TokioCommand;
use watchexec_signals::Signal;

use crate::{command::Command, errors::SyncIoError, flag::Flag};

use super::{
	messages::{Control, ControlMessage, Ticket},
	priority::{Priority, PrioritySender, SendError},
	JobTaskContext,
};

/// A handle to a job task spawned in the supervisor.
///
/// A job is a task which manages a [`Command`]. It is responsible for spawning the command's
/// program, for handling messages which control it, for managing the program's lifetime, and for
/// collecting its exit status and some timing information.
///
/// Most of the methods here queue [`Control`]s to the job task and return [`Ticket`]s. Controls
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
	pub(crate) control_queue: PrioritySender,

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

	fn send_controls<const N: usize>(
		&self,
		controls: [Control; N],
		priority: Priority,
	) -> Result<Ticket, SendError> {
		if self.gone.raised() {
			Ok(Ticket::cancelled())
		} else if N == 1 {
			let control = controls.into_iter().next().unwrap();
			let (ticket, control) = self.prepare_control(control);
			self.control_queue.send(control, priority)?;
			Ok(ticket)
		} else {
			let (mut tickets, controls): (Vec<Ticket>, Vec<ControlMessage>) = controls
				.into_iter()
				.map(|control| self.prepare_control(control))
				.unzip();
			let ticket = tickets.pop().expect("controls should always be non-empty");

			for control in controls {
				self.control_queue.send(control, priority)?;
			}
			Ok(ticket)
		}
	}

	/// Send a control message to the command.
	///
	/// All control messages are queued in the order they're sent and processed in order.
	///
	/// In general prefer using the other methods on this struct rather than sending [`Control`]s
	/// directly.
	pub fn control(&self, control: Control) -> Result<Ticket, SendError> {
		self.send_controls([control], Priority::Normal)
	}

	/// Start the command if it's not running.
	pub fn start(&self) -> Result<Ticket, SendError> {
		self.control(Control::Start)
	}

	/// Stop the command if it's running.
	pub fn stop(&self) -> Result<Ticket, SendError> {
		self.control(Control::Stop)
	}

	/// Gracefully stop the command if it's running.
	///
	/// If `grace > Duration::ZERO`, the command will be sent `signal` and then given `grace` time
	/// before being forcefully terminated.
	///
	/// On Windows, this is equivalent to [`stop`](Job::stop).
	pub fn stop_with_signal(&self, signal: Signal, grace: Duration) -> Result<Ticket, SendError> {
		if cfg!(unix) {
			self.control(Control::GracefulStop { signal, grace })
		} else {
			self.stop()
		}
	}

	/// Restart the command if it's running, or start it if it's not.
	pub fn restart(&self) -> Result<Ticket, SendError> {
		self.send_controls([Control::Stop, Control::Start], Priority::Normal)
	}

	/// Gracefully restart the command if it's running, or start it if it's not.
	///
	/// If `grace > Duration::ZERO`, the command will be sent `signal` and then given `grace` time
	/// before being forcefully terminated.
	///
	/// On Windows, this is equivalent to [`restart`](Job::restart).
	pub fn restart_with_signal(
		&self,
		signal: Signal,
		grace: Duration,
	) -> Result<Ticket, SendError> {
		if cfg!(unix) {
			self.send_controls(
				[Control::GracefulStop { signal, grace }, Control::Start],
				Priority::Normal,
			)
		} else {
			self.restart()
		}
	}

	/// Restart the command if it's running, but don't start it if it's not.
	pub fn try_restart(&self) -> Result<Ticket, SendError> {
		self.control(Control::TryRestart)
	}

	/// Restart the command if it's running, but don't start it if it's not.
	///
	/// If `grace > Duration::ZERO`, the command will be sent `signal` and then given `grace` time
	/// before being forcefully terminated.
	///
	/// On Windows, this is equivalent to [`try_restart`](Job::try_restart).
	pub fn try_restart_with_signal(
		&self,
		signal: Signal,
		grace: Duration,
	) -> Result<Ticket, SendError> {
		if cfg!(unix) {
			self.control(Control::TryGracefulRestart { signal, grace })
		} else {
			self.try_restart()
		}
	}

	/// Send a signal to the command.
	///
	/// Sends a signal to the current program, if there is one. If there isn't, this is a no-op.
	///
	/// On Windows, this is a no-op.
	pub fn signal(&self, sig: Signal) -> Result<Ticket, SendError> {
		if cfg!(unix) {
			self.control(Control::Signal(sig))
		} else {
			Ok(Ticket::cancelled())
		}
	}

	/// Stop the command, then mark it for garbage collection.
	///
	/// The underlying control messages are sent like normal, so they wait for all pending controls
	/// to process. If you want to delete the command immediately, use `delete_now()`.
	pub fn delete(&self) -> Result<Ticket, SendError> {
		self.send_controls([Control::Stop, Control::Delete], Priority::Normal)
	}

	/// Stop the command immediately, then mark it for garbage collection.
	///
	/// The underlying control messages are sent with higher priority than normal, so they bypass
	/// all others. If you want to delete after all current controls are processed, use `delete()`.
	pub fn delete_now(&self) -> Result<Ticket, SendError> {
		self.send_controls([Control::Stop, Control::Delete], Priority::Urgent)
	}

	/// Get a future which resolves when the command ends.
	///
	/// If the command is not running, the future resolves immediately.
	///
	/// The underlying control message is sent with higher priority than normal, so it targets the
	/// actively running command, not the one that will be running after the rest of the controls
	/// get done; note that may still be racy if the command ends between the time the message is
	/// sent and the time it's processed.
	pub fn to_wait(&self) -> Result<Ticket, SendError> {
		self.send_controls([Control::NextEnding], Priority::High)
	}

	/// Run an arbitrary function.
	///
	/// The function is given [`&JobTaskContext`](JobTaskContext), which contains the state of the
	/// currently executing, next-to-start, or just-finished command, as well as the final state of
	/// the _previous_ run of the command.
	///
	/// Technically, some operations can be done through a `&self` shared borrow on the running
	/// command's [`ErasedChild`](command_group::tokio::ErasedChild), but this library recommends
	/// against taking advantage of this, and prefer using the methods here instead, so that the
	/// supervisor can keep track of what's going on.
	pub fn run(
		&self,
		fun: impl FnOnce(&JobTaskContext) + Send + Sync + 'static,
	) -> Result<Ticket, SendError> {
		self.control(Control::SyncFunc(Box::new(fun)))
	}

	/// Run an arbitrary function and await the returned future.
	///
	/// The function is given [`&JobTaskContext`](JobTaskContext), which contains the state of the
	/// currently executing, next-to-start, or just-finished command, as well as the final state of
	/// the _previous_ run of the command.
	///
	/// Technically, some operations can be done through a `&self` shared borrow on the running
	/// command's [`ErasedChild`](command_group::tokio::ErasedChild), but this library recommends
	/// against taking advantage of this, and prefer using the methods here instead, so that the
	/// supervisor can keep track of what's going on.
	pub fn run_async(
		&self,
		fun: impl (FnOnce(&JobTaskContext) -> Box<dyn Future<Output = ()> + Send + Sync>)
			+ Send
			+ Sync
			+ 'static,
	) -> Result<Ticket, SendError> {
		self.control(Control::AsyncFunc(Box::new(fun)))
	}

	/// Set the spawn hook.
	///
	/// The hook will be called once per process spawned, before the process is spawned. It's given
	/// a mutable reference to the [`tokio::process::Command`] and some context; it can modify the
	/// command as it sees fit.
	pub fn set_spawn_hook(
		&self,
		fun: impl Fn(&mut TokioCommand, &JobTaskContext) + Send + Sync + 'static,
	) -> Result<Ticket, SendError> {
		self.control(Control::SetSyncSpawnHook(Arc::new(fun)))
	}

	/// Set the spawn hook (async version).
	///
	/// The hook will be called once per process spawned, before the process is spawned. It's given
	/// a mutable reference to the [`tokio::process::Command`] and some context; it can modify the
	/// command as it sees fit.
	pub fn set_spawn_async_hook(
		&self,
		fun: impl (Fn(&mut TokioCommand, &JobTaskContext) -> Box<dyn Future<Output = ()> + Send + Sync>)
			+ Send
			+ Sync
			+ 'static,
	) -> Result<Ticket, SendError> {
		self.control(Control::SetAsyncSpawnHook(Arc::new(fun)))
	}

	/// Unset any spawn hook.
	pub fn unset_spawn_hook(&self) -> Result<Ticket, SendError> {
		self.control(Control::UnsetSpawnHook)
	}

	/// Set the error handler.
	pub fn set_error_handler(
		&self,
		fun: impl Fn(SyncIoError) + Send + Sync + 'static,
	) -> Result<Ticket, SendError> {
		self.control(Control::SetSyncErrorHandler(Arc::new(fun)))
	}

	/// Set the error handler (async version).
	pub fn set_async_error_handler(
		&self,
		fun: impl (Fn(SyncIoError) -> Box<dyn Future<Output = ()> + Send + Sync>)
			+ Send
			+ Sync
			+ 'static,
	) -> Result<Ticket, SendError> {
		self.control(Control::SetAsyncErrorHandler(Arc::new(fun)))
	}

	/// Unset the error handler.
	///
	/// Errors will be silently ignored.
	pub fn unset_error_handler(&self) -> Result<Ticket, SendError> {
		self.control(Control::UnsetErrorHandler)
	}
}
