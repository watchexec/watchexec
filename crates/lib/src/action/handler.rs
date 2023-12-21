use std::{collections::HashMap, path::Path, sync::Arc, time::Duration};
use tokio::task::JoinHandle;
use watchexec_events::{Event, FileType, ProcessEnd};
use watchexec_signals::Signal;
use watchexec_supervisor::{
	command::Command,
	job::{start_job, Job},
};

use crate::id::Id;

use super::QuitManner;

/// The environment given to the action handler.
///
/// The action handler is the heart of a Watchexec program. Within, you decide what happens when an
/// event successfully passes all filters. Watchexec maintains a set of Supervised [`Job`]s, which
/// are assigned a unique [`Id`] for lightweight reference. In this action handler, you should
/// add commands to be supervised with `create_job()`, or find an already-supervised job with
/// `get_job()` or `list_jobs()`. You can interact with jobs directly via their handles, and can
/// even store clones of the handles for later use outside the action handler.
///
/// The action handler is also given the [`Event`]s which triggered the action. These are expected
/// to be the way to determine what to do with a job. However, in some applications you might not
/// care about them, and that's fine too: for example, you can build a Watchexec which only does
/// process supervision, and is triggered entirely by synthetic events. Conversely, you are also not
/// obligated to use the job handles: you can build a Watchexec which only does something with the
/// events, and never actually starts any processes.
///
/// There are some important considerations to keep in mind when writing an action handler:
///
/// 1. The action handler is called with the supervisor set _as of when the handler was called_.
///    This is particularly important when multiple action handlers might be running at the same
///    time: they might have incomplete views of the supervisor set.
///
/// 2. The way the action handler communicates with the Watchexec handler is through the return
///    value of the handler. That is, when you add a job with `create_job()`, the job is not added
///    to the Watchexec instance's supervisor set until the action handler returns. Similarly, when
///    using `quit()`, the quit action is not performed until the action handler returns and the
///    Watchexec instance is able to see it.
///
/// 3. The action handler blocks the action main loop. This means that if you have a long-running
///    action handler, the Watchexec instance will not be able to process events until the handler
///    returns. That will cause events to accumulate and then get dropped once the channel reaches
///    capacity, which will impact your ability to receive signals (such as a Ctrl-C), and may spew
///    [`EventChannelTrySend` errors](crate::error::RuntimeError::EventChannelTrySend).
///
///    If you want to do something long-running, you should either ignore that error, and accept
///    events may be dropped, or preferrably spawn a task to do it, and return from the action
///    handler as soon as possible.
#[derive(Debug)]
pub struct Handler {
	/// The collected events which triggered the action.
	pub events: Arc<[Event]>,
	extant: HashMap<Id, Job>,
	pub(crate) new: HashMap<Id, (Job, JoinHandle<()>)>,
	pub(crate) quit: Option<QuitManner>,
}

impl Handler {
	pub(crate) fn new(events: Arc<[Event]>, jobs: HashMap<Id, Job>) -> Self {
		Self {
			events,
			extant: jobs,
			new: HashMap::new(),
			quit: None,
		}
	}

	/// Create a new job and return its handle.
	///
	/// This starts the [`Job`] immediately, and stores a copy of its handle and [`Id`] in this
	/// `Action` (and thus in the Watchexec instance, when the action handler returns).
	pub fn create_job(&mut self, command: Arc<Command>) -> (Id, Job) {
		let id = Id::default();
		let (job, task) = start_job(command);
		self.new.insert(id, (job.clone(), task));
		(id, job)
	}

	// exposing this is dangerous as it allows duplicate IDs which may leak jobs
	fn create_job_with_id(&mut self, id: Id, command: Arc<Command>) -> Job {
		let (job, task) = start_job(command);
		self.new.insert(id, (job.clone(), task));
		job
	}

	/// Get an existing job or create a new one given an Id.
	///
	/// This starts the [`Job`] immediately if one with the Id doesn't exist, and stores a copy of
	/// its handle and [`Id`] in this `Action` (and thus in the Watchexec instance, when the action
	/// handler returns).
	pub fn get_or_create_job(&mut self, id: Id, command: impl Fn() -> Arc<Command>) -> Job {
		self.get_job(id)
			.unwrap_or_else(|| self.create_job_with_id(id, command()))
	}

	/// Get a job given its Id.
	///
	/// This returns a job handle, if it existed when this handler was called.
  #[must_use]
	pub fn get_job(&self, id: Id) -> Option<Job> {
		self.extant.get(&id).cloned()
	}

	/// List all jobs currently supervised by Watchexec.
	///
	/// This returns an iterator over all jobs, in no particular order, as of when this handler was
	/// called.
	pub fn list_jobs(&self) -> impl Iterator<Item = (Id, Job)> + '_ {
		self.extant.iter().map(|(id, job)| (*id, job.clone()))
	}

	/// Shut down the Watchexec instance immediately.
	///
	/// This will kill and drop all jobs without waiting on processes, then quit.
	///
	/// Use `graceful_quit()` to wait for processes to finish before quitting.
	///
	/// The quit is initiated once the action handler returns, not when this method is called.
	pub fn quit(&mut self) {
		self.quit = Some(QuitManner::Abort);
	}

	/// Shut down the Watchexec instance gracefully.
	///
	/// This will send graceful stops to all jobs, wait on them to finish, then reap them and quit.
	///
	/// Use `quit()` to quit more abruptly.
	///
	/// If you want to wait for all other actions to finish and for jobs to get cleaned up, but not
	/// gracefully delay for processes, you can do:
	///
	/// ```no_compile
	/// action.quit_gracefully(Signal::ForceStop, Duration::ZERO);
	/// ```
	///
	/// The quit is initiated once the action handler returns, not when this method is called.
	pub fn quit_gracefully(&mut self, signal: Signal, grace: Duration) {
		self.quit = Some(QuitManner::Graceful { signal, grace });
	}

	/// Convenience to get all signals in the event set.
	pub fn signals(&self) -> impl Iterator<Item = Signal> + '_ {
		self.events.iter().flat_map(Event::signals)
	}

	/// Convenience to get all paths in the event set.
	///
	/// An action contains a set of events, and some of those events might relate to watched
	/// files, and each of *those* events may have one or more paths that were affected.
	/// To hide this complexity this method just provides any and all paths in the event,
	/// along with the type of file at that path, if Watchexec knows that.
	pub fn paths(&self) -> impl Iterator<Item = (&Path, Option<&FileType>)> + '_ {
		self.events.iter().flat_map(Event::paths)
	}

	/// Convenience to get all process completions in the event set.
	pub fn completions(&self) -> impl Iterator<Item = Option<ProcessEnd>> + '_ {
		self.events.iter().flat_map(Event::completions)
	}
}
