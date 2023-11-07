use std::{time::Duration, sync::{Arc, atomic::AtomicBool}};

use command_group::Signal;

use crate::command::Command;

use super::{priority::Priority, control::Control, jobid::JobId};

type ControlResult = Result<(), async_priority_channel::SendError<(Control, Priority)>>;

#[derive(Debug, Clone)]
pub struct Job {
	id: JobId,
	command: Arc<Command>,
	control_queue: async_priority_channel::Sender<Control, Priority>,

	/// Set to true when this struct is no longer needed and can be dropped.
	gc: Arc<AtomicBool>,
}

impl Job {
	pub fn id(&self) -> JobId {
		self.id
	}

	/// Send a control message to the command.
	///
	/// All control messages are queued in the order they're sent and processed in order.
	pub async fn control(&self, msg: Control) -> ControlResult {
		self.control_queue.send(msg, Priority::Normal).await
	}

	/// Send a signal to the command.
	pub async fn signal(&self, sig: Signal) -> ControlResult {
		self.control(Control::Signal(sig)).await
	}

	/// Wait for the command to exit, with an optional timeout.
	pub async fn wait(&self, timeout: Option<Duration>) -> ControlResult {
		self.control(Control::Wait(timeout)).await
	}

	/// Stop the command if it's running.
	///
	/// If `grace > Duration::ZERO`, the command will be sent `signal` and then given `grace` time
	/// before being forcefully terminated.
	pub async fn stop(&self, signal: Signal, grace: Duration) -> ControlResult {
		self.control(Control::Stop { signal, grace }).await
	}

	/// Start the command if it's not running.
	pub async fn start(&self) -> ControlResult {
		self.control(Control::Start).await
	}

	/// Start the command, using the given pre-spawn hook.
	pub async fn start_with_hook(&self, fun: impl FnOnce() + Send + 'static) -> ControlResult {
		self.control(Control::StartWithHook(Box::new(fun)))
			.await
	}

	/// Restart the command if it's running, or start it if it's not.
	pub async fn restart(&self, signal: Signal, grace: Duration) -> ControlResult {
		self.stop(signal, grace).await?;
		self.start().await
	}

	/// Restart the command if it's running, but don't start it if it's not.
	pub async fn try_restart(&self, signal: Signal, grace: Duration) -> ControlResult {
		self.control(Control::TryRestart { signal, grace }).await
	}

	/// Run a hook.
	pub async fn run_hook(&self, fun: impl FnOnce() + Send + 'static) -> ControlResult {
		self.control(Control::Hook(Box::new(fun))).await
	}

	async fn delete_with(&self, priority: Priority, signal: Signal, grace: Duration) -> ControlResult {
		self.control_queue
			.send(Control::Stop { signal, grace }, priority)
			.await?;
		self.control_queue
			.send(Control::Delete(self.gc.clone()), priority)
			.await
	}

	/// Stop the command immediately, then mark it for garbage collection.
	///
	/// The underlying control messages are sent like normal, so they wait for all pending controls
	/// to process. If you want to delete the command immediately, use `delete_now()`.
	///
	/// The arguments are the same as for `stop()`.
	pub async fn delete(&self, signal: Signal, grace: Duration) -> ControlResult {
		self.delete_with(Priority::Normal, signal, grace).await
	}

	/// Stop the command immediately, then mark it for garbage collection.
	///
	/// The underlying control messages are sent with higher priority than normal, so they bypass
	/// all others. If you want to delete after all current controls are processed, use `delete()`.
	///
	/// The arguments are the same as for `stop()`.
	pub async fn delete_now(&self, signal: Signal, grace: Duration) -> ControlResult {
		self.delete_with(Priority::Urgent, signal, grace).await
	}
}

impl PartialEq for Job {
	fn eq(&self, other: &Self) -> bool {
		self.id == other.id
	}
}
