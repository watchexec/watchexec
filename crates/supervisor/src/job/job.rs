#![allow(clippy::must_use_candidate)] // Ticket-returning methods are supposed to be used without awaiting

use std::{future::Future, sync::Arc, time::Duration};

use process_wrap::tokio::CommandWrap;
use watchexec_signals::Signal;

use crate::{command::Command, errors::SyncIoError, flag::Flag};

use super::{
	messages::{Control, ControlMessage, Ticket},
	priority::{Priority, PrioritySender},
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
///
/// This struct is cloneable (internally it is made of Arcs). Dropping the last instance of a Job
/// will close the job's control queue, which will cause the job task to stop gracefully. Note that
/// a task graceful stop is not the same as a graceful stop of the contained command; when the job
/// drops, the command will be dropped in turn, and forcefully terminated via `kill_on_drop`.
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

	/// If this job is dead.
	pub fn is_dead(&self) -> bool {
		self.gone.raised()
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

	pub(crate) fn send_controls<const N: usize>(
		&self,
		controls: [Control; N],
		priority: Priority,
	) -> Ticket {
		if N == 0 || self.gone.raised() {
			Ticket::cancelled()
		} else if N == 1 {
			let control = controls.into_iter().next().expect("UNWRAP: N > 0");
			let (ticket, control) = self.prepare_control(control);
			self.control_queue.send(control, priority);
			ticket
		} else {
			let mut last_ticket = None;
			for control in controls {
				let (ticket, control) = self.prepare_control(control);
				last_ticket = Some(ticket);
				self.control_queue.send(control, priority);
			}
			last_ticket.expect("UNWRAP: N > 0")
		}
	}

	/// Send a control message to the command.
	///
	/// All control messages are queued in the order they're sent and processed in order.
	///
	/// In general prefer using the other methods on this struct rather than sending [`Control`]s
	/// directly.
	pub fn control(&self, control: Control) -> Ticket {
		self.send_controls([control], Priority::Normal)
	}

	/// Start the command if it's not running.
	pub fn start(&self) -> Ticket {
		self.control(Control::Start)
	}

	/// Stop the command if it's running and wait for completion.
	///
	/// If you don't want to wait for completion, use `signal(Signal::ForceStop)` instead.
	pub fn stop(&self) -> Ticket {
		self.control(Control::Stop)
	}

	/// Gracefully stop the command if it's running.
	///
	/// The command will be sent `signal` and then given `grace` time before being forcefully
	/// terminated. If `grace` is zero, that still happens, but the command is terminated forcefully
	/// on the next "tick" of the supervisor loop, which doesn't leave the process a lot of time to
	/// do anything.
	pub fn stop_with_signal(&self, signal: Signal, grace: Duration) -> Ticket {
		if cfg!(unix) {
			self.control(Control::GracefulStop { signal, grace })
		} else {
			self.stop()
		}
	}

	/// Restart the command if it's running, or start it if it's not.
	pub fn restart(&self) -> Ticket {
		self.send_controls([Control::Stop, Control::Start], Priority::Normal)
	}

	/// Gracefully restart the command if it's running, or start it if it's not.
	///
	/// The command will be sent `signal` and then given `grace` time before being forcefully
	/// terminated. If `grace` is zero, that still happens, but the command is terminated forcefully
	/// on the next "tick" of the supervisor loop, which doesn't leave the process a lot of time to
	/// do anything.
	pub fn restart_with_signal(&self, signal: Signal, grace: Duration) -> Ticket {
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
	pub fn try_restart(&self) -> Ticket {
		self.control(Control::TryRestart)
	}

	/// Restart the command if it's running, but don't start it if it's not.
	///
	/// The command will be sent `signal` and then given `grace` time before being forcefully
	/// terminated. If `grace` is zero, that still happens, but the command is terminated forcefully
	/// on the next "tick" of the supervisor loop, which doesn't leave the process a lot of time to
	/// do anything.
	pub fn try_restart_with_signal(&self, signal: Signal, grace: Duration) -> Ticket {
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
	/// On Windows, this is a no-op for all signals but [`Signal::ForceStop`], which tries to stop
	/// the command like a `stop()` would, but doesn't wait for completion. This is because Windows
	/// doesn't have signals; in future [`Hangup`](Signal::Hangup), [`Interrupt`](Signal::Interrupt),
	/// and [`Terminate`](Signal::Terminate) may be implemented using [GenerateConsoleCtrlEvent],
	/// see [tracking issue #219](https://github.com/watchexec/watchexec/issues/219).
	///
	/// [GenerateConsoleCtrlEvent]: https://learn.microsoft.com/en-us/windows/console/generateconsolectrlevent
	pub fn signal(&self, sig: Signal) -> Ticket {
		self.control(Control::Signal(sig))
	}

	/// Stop the command, then mark it for garbage collection.
	///
	/// The underlying control messages are sent like normal, so they wait for all pending controls
	/// to process. If you want to delete the command immediately, use `delete_now()`.
	pub fn delete(&self) -> Ticket {
		self.send_controls([Control::Stop, Control::Delete], Priority::Normal)
	}

	/// Stop the command immediately, then mark it for garbage collection.
	///
	/// The underlying control messages are sent with higher priority than normal, so they bypass
	/// all others. If you want to delete after all current controls are processed, use `delete()`.
	pub fn delete_now(&self) -> Ticket {
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
	pub fn to_wait(&self) -> Ticket {
		self.send_controls([Control::NextEnding], Priority::High)
	}

	/// Run an arbitrary function.
	///
	/// The function is given [`&JobTaskContext`](JobTaskContext), which contains the state of the
	/// currently executing, next-to-start, or just-finished command, as well as the final state of
	/// the _previous_ run of the command.
	///
	/// Technically, some operations can be done through a `&self` shared borrow on the running
	/// command's [`ChildWrapper`], but this library recommends against taking advantage of this,
	/// and prefer using the methods on here instead, so that the supervisor can keep track of
	/// what's going on.
	pub fn run(&self, fun: impl FnOnce(&JobTaskContext<'_>) + Send + Sync + 'static) -> Ticket {
		self.control(Control::SyncFunc(Box::new(fun)))
	}

	/// Run an arbitrary function and await the returned future.
	///
	/// The function is given [`&JobTaskContext`](JobTaskContext), which contains the state of the
	/// currently executing, next-to-start, or just-finished command, as well as the final state of
	/// the _previous_ run of the command.
	///
	/// Technically, some operations can be done through a `&self` shared borrow on the running
	/// command's [`ChildWrapper`], but this library recommends against taking advantage of this,
	/// and prefer using the methods on here instead, so that the supervisor can keep track of
	/// what's going on.
	///
	/// A gotcha when using this method is that the future returned by the function can live longer
	/// than the `&JobTaskContext` it was given, so you can't bring the context into the async block
	/// and instead must clone or copy the parts you need beforehand, in the sync portion.
	///
	/// For example, this won't compile:
	///
	/// ```compile_fail
	/// # use std::sync::Arc;
	/// # use tokio::sync::mpsc;
	/// # use watchexec_supervisor::command::{Command, Program};
	/// # use watchexec_supervisor::job::{CommandState, start_job};
	/// #
	/// # let (job, _task) = start_job(Arc::new(Command { program: Program::Exec { prog: "/bin/date".into(), args: Vec::new() }.into(), options: Default::default() }));
	/// let (channel, receiver) = mpsc::channel(10);
	/// job.run_async(|context| Box::new(async move {
	///     if let CommandState::Finished { status, .. } = context.current {
	///         channel.send(status).await.ok();
	///     }
	/// }));
	/// ```
	///
	/// But this does:
	///
	/// ```no_run
	/// # use std::sync::Arc;
	/// # use tokio::sync::mpsc;
	/// # use watchexec_supervisor::command::{Command, Program};
	/// # use watchexec_supervisor::job::{CommandState, start_job};
	/// #
	/// # let (job, _task) = start_job(Arc::new(Command { program: Program::Exec { prog: "/bin/date".into(), args: Vec::new() }.into(), options: Default::default() }));
	/// let (channel, receiver) = mpsc::channel(10);
	/// job.run_async(|context| {
	///     let status = if let CommandState::Finished { status, .. } = context.current {
	///         Some(*status)
	///     } else {
	///         None
	///     };
	///
	///     Box::new(async move {
	///         if let Some(status) = status {
	///             channel.send(status).await.ok();
	///         }
	///     })
	/// });
	/// ```
	pub fn run_async(
		&self,
		fun: impl (FnOnce(&JobTaskContext<'_>) -> Box<dyn Future<Output = ()> + Send + Sync>)
			+ Send
			+ Sync
			+ 'static,
	) -> Ticket {
		self.control(Control::AsyncFunc(Box::new(fun)))
	}

	/// Set the spawn hook.
	///
	/// The hook will be called once per process spawned, before the process is spawned. It's given
	/// a mutable reference to the [`process_wrap::tokio::CommandWrap`] and some context; it
	/// can modify or further [wrap](process_wrap) the command as it sees fit.
	pub fn set_spawn_hook(
		&self,
		fun: impl Fn(&mut CommandWrap, &JobTaskContext<'_>) + Send + Sync + 'static,
	) -> Ticket {
		self.control(Control::SetSyncSpawnHook(Arc::new(fun)))
	}

	/// Set the spawn hook (async version).
	///
	/// The hook will be called once per process spawned, before the process is spawned. It's given
	/// a mutable reference to the [`process_wrap::tokio::CommandWrap`] and some context; it
	/// can modify or further [wrap](process_wrap) the command as it sees fit.
	///
	/// A gotcha when using this method is that the future returned by the function can live longer
	/// than the references it was given, so you can't bring the command or context into the async
	/// block and instead must clone or copy the parts you need beforehand, in the sync portion. See
	/// the documentation for [`run_async`](Job::run_async) for an example.
	///
	/// Fortunately, async spawn hooks should be exceedingly rare: there's very few things to do in
	/// spawn hooks that can't be done in the simpler sync version.
	pub fn set_spawn_async_hook(
		&self,
		fun: impl (Fn(&mut CommandWrap, &JobTaskContext<'_>) -> Box<dyn Future<Output = ()> + Send + Sync>)
			+ Send
			+ Sync
			+ 'static,
	) -> Ticket {
		self.control(Control::SetAsyncSpawnHook(Arc::new(fun)))
	}

	/// Unset any spawn hook.
	pub fn unset_spawn_hook(&self) -> Ticket {
		self.control(Control::UnsetSpawnHook)
	}

	/// Set the error handler.
	pub fn set_error_handler(&self, fun: impl Fn(SyncIoError) + Send + Sync + 'static) -> Ticket {
		self.control(Control::SetSyncErrorHandler(Arc::new(fun)))
	}

	/// Set the error handler (async version).
	pub fn set_async_error_handler(
		&self,
		fun: impl (Fn(SyncIoError) -> Box<dyn Future<Output = ()> + Send + Sync>)
			+ Send
			+ Sync
			+ 'static,
	) -> Ticket {
		self.control(Control::SetAsyncErrorHandler(Arc::new(fun)))
	}

	/// Unset the error handler.
	///
	/// Errors will be silently ignored.
	pub fn unset_error_handler(&self) -> Ticket {
		self.control(Control::UnsetErrorHandler)
	}
}
