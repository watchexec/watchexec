//! Event source for changes to files and directories.
//!
//! # Recursive source filtering
//!
//! Watchexec does its own recursive traversal, using non-recursive watches on supported backends
//! and filtering the watch tree from the [`Filterer`] implementation.
//!
//! Watched paths are themselves never filtered.
//!
//! Watch and scan failures are reported while traversal continues with independent paths,
//! rebuilding the watcher from known registrations when a failed operation may have changed backend
//! state.
//!
//! `FSEvents` has no option to watch non-recursively, but also watches an entire tree directly, so
//! we don't try to do source subtree filtering; filtering is covered at event-level.
//!
//! If Notify recommends Kqueue, Watchexec uses Poll instead.

mod backend;
mod recursor;

use std::{
	collections::{HashMap, HashSet, VecDeque},
	fs::metadata,
	mem::take,
	path::PathBuf,
	sync::{
		atomic::{AtomicU64, Ordering},
		Arc, Mutex,
	},
	time::Duration,
};

use async_priority_channel as priority;
use normalize_path::NormalizePath;
use notify::{
	event::{ModifyKind, RenameMode},
	EventKind,
};
use tokio::sync::mpsc;
use tracing::{debug, trace};
use watchexec_events::{Event, Priority, Source, Tag};

use crate::{
	error::{CriticalError, FsWatcherError, RuntimeError},
	filter::Filterer,
	Config,
};

use self::{
	backend::{classify_resource_error, create_notify, Backend},
	recursor::{FsScanner, Recursor},
};

// re-export for compatibility, until next major version
pub use crate::WatchedPath;

/// What kind of filesystem watcher to use.
///
/// For now only native and poll watchers are supported. In the future there may be additional
/// watchers available on some platforms.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Watcher {
	/// The Notify-recommended watcher on the platform.
	///
	/// For platforms Notify supports, that's a [native implementation][notify::RecommendedWatcher],
	/// for others it's polling with a default interval.
	///
	/// If Notify recommends Kqueue, Watchexec uses Poll instead.
	#[default]
	Native,

	/// Notify’s [poll watcher][notify::PollWatcher] with a custom interval.
	Poll(Duration),
}

impl Watcher {
	fn create(
		self,
		follow_symlinks: bool,
		f: impl notify::EventHandler,
	) -> Result<Box<dyn Backend>, CriticalError> {
		create_notify(self, follow_symlinks, f).map_err(|error| CriticalError::FsWatcherInit {
			kind: self,
			err: match classify_resource_error(&error) {
				Some(resource) => resource.into_fs_error(error),
				None => FsWatcherError::Create(error),
			},
		})
	}

	fn backend_kind(self) -> notify::WatcherKind {
		self.backend_kind_for(<notify::RecommendedWatcher as notify::Watcher>::kind())
	}

	const fn backend_kind_for(self, recommended: notify::WatcherKind) -> notify::WatcherKind {
		match self {
			Self::Native => match recommended {
				notify::WatcherKind::Kqueue => notify::WatcherKind::PollWatcher,
				_ => recommended,
			},
			Self::Poll(_) => notify::WatcherKind::PollWatcher,
		}
	}

	fn recursion_strategy(self) -> RecursionStrategy {
		recursion_strategy(self.backend_kind())
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecursionStrategy {
	PerDirectory,
	RecursiveRoots,
	NotifyOwned,
}

impl RecursionStrategy {
	const fn owns_logical_tree(self) -> bool {
		matches!(self, Self::PerDirectory | Self::RecursiveRoots)
	}
}

const fn recursion_strategy(kind: notify::WatcherKind) -> RecursionStrategy {
	match kind {
		notify::WatcherKind::Inotify
		| notify::WatcherKind::ReadDirectoryChangesWatcher
		| notify::WatcherKind::PollWatcher => RecursionStrategy::PerDirectory,
		notify::WatcherKind::Fsevent => RecursionStrategy::RecursiveRoots,
		_ => RecursionStrategy::NotifyOwned,
	}
}

const fn strategy_requires_recreation(
	strategy: RecursionStrategy,
	roots_changed: bool,
	needs_retry: bool,
) -> bool {
	match strategy {
		RecursionStrategy::PerDirectory => false,
		RecursionStrategy::RecursiveRoots | RecursionStrategy::NotifyOwned => {
			roots_changed || needs_retry
		}
	}
}

/// Collect synthetic create events for the current contents of the given watch roots.
///
/// The events are returned to the caller and are not sent through Watchexec's action pipeline.
/// Source-directory traversal uses the same recursor machinery as filesystem watching, including
/// path normalisation, symlink policy, and source-directory filtering.
pub fn collect_initial_events(
	pathset: &[WatchedPath],
	watcher: Watcher,
	follow_symlinks: bool,
	filter: Arc<dyn Filterer>,
) -> Result<(Vec<Event>, Vec<RuntimeError>), CriticalError> {
	let cwd = std::env::current_dir().map_err(|err| CriticalError::IoError {
		about: "obtaining current directory for initial filesystem event collection",
		err,
	})?;

	Ok(Recursor::collect_initial_events(
		pathset,
		watcher,
		follow_symlinks,
		cwd,
		filter,
	))
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Topology {
	Create(PathBuf),
	Remove(PathBuf),
	Rename { from: PathBuf, to: PathBuf },
	Ambiguous(PathBuf),
	Rescan,
}

#[derive(Debug)]
enum ControlMessage {
	Topology {
		generation: u64,
		sequence: u64,
		changes: Vec<Topology>,
	},
	Error {
		watcher: Watcher,
		error: notify::Error,
	},
}

#[derive(Debug)]
struct OrdinaryMessage {
	generation: u64,
	topology_sequence: u64,
	event: notify::Event,
}

#[derive(Default)]
struct CallbackFence {
	generation: AtomicU64,
	topology_enqueued: AtomicU64,
	serial: Mutex<()>,
}

#[derive(Clone, Copy, Debug)]
struct PendingReady {
	topology_fence: Option<u64>,
}

fn dispatch_callback_event(
	generation: u64,
	watcher: Watcher,
	control: &mpsc::UnboundedSender<ControlMessage>,
	ordinary: &mpsc::Sender<OrdinaryMessage>,
	callback_fence: &CallbackFence,
	event: Result<notify::Event, notify::Error>,
) {
	if callback_fence.generation.load(Ordering::Acquire) != generation {
		return;
	}
	trace!(?event, "receiving possible event from watcher");
	let changes = if watcher.recursion_strategy().owns_logical_tree() {
		event.as_ref().ok().map(topology_messages)
	} else {
		None
	};
	let Ok(_serial) = callback_fence.serial.lock() else {
		return;
	};
	if callback_fence.generation.load(Ordering::Acquire) != generation {
		return;
	}

	match event {
		Ok(event) => {
			let changes = changes.unwrap_or_default();
			let topology_sequence = if changes.is_empty() {
				callback_fence.topology_enqueued.load(Ordering::Acquire)
			} else {
				let sequence = callback_fence
					.topology_enqueued
					.fetch_add(1, Ordering::AcqRel)
					.wrapping_add(1);
				if control
					.send(ControlMessage::Topology {
						generation,
						sequence,
						changes,
					})
					.is_err()
				{
					return;
				}
				sequence
			};
			match ordinary.try_send(OrdinaryMessage {
				generation,
				topology_sequence,
				event,
			}) {
				Err(mpsc::error::TrySendError::Full(_)) => {
					debug!("filesystem callback event lane is full; dropping ordinary event");
				}
				Ok(()) | Err(mpsc::error::TrySendError::Closed(_)) => {}
			}
		}
		Err(error) => {
			let _ = control.send(ControlMessage::Error { watcher, error });
		}
	}
}

struct AppliedConfig {
	pathset: HashSet<WatchedPath>,
	watcher: Watcher,
	follow_symlinks: bool,
	filter: Arc<dyn Filterer>,
}

struct WorkerState {
	recursor: Option<Recursor>,
	applied: Option<AppliedConfig>,
	generation: u64,
	cwd: PathBuf,
	control: mpsc::UnboundedSender<ControlMessage>,
	ordinary: mpsc::Sender<OrdinaryMessage>,
	callback_fence: Arc<CallbackFence>,
	events: priority::Sender<Event, Priority>,
	processed_topology: u64,
	retained_generations: HashSet<u64>,
	pending_ready: Option<PendingReady>,
}

impl WorkerState {
	fn new(
		cwd: PathBuf,
		control: mpsc::UnboundedSender<ControlMessage>,
		ordinary: mpsc::Sender<OrdinaryMessage>,
		callback_fence: Arc<CallbackFence>,
		events: priority::Sender<Event, Priority>,
	) -> Self {
		Self {
			recursor: None,
			applied: None,
			generation: 0,
			cwd,
			control,
			ordinary,
			callback_fence,
			events,
			processed_topology: 0,
			retained_generations: HashSet::new(),
			pending_ready: None,
		}
	}

	fn apply_config(&mut self, config: &Config) -> Result<(), CriticalError> {
		let pathset = config.pathset.get();
		let normalized_pathset: HashSet<_> = pathset
			.iter()
			.map(|path| WatchedPath {
				path: if path.path.is_absolute() {
					path.path.normalize()
				} else {
					self.cwd.join(&path.path).normalize()
				},
				recursive: path.recursive,
			})
			.collect();
		let watcher = config.file_watcher.get();
		let strategy = watcher.recursion_strategy();
		let follow_symlinks = config.follow_symlinks.get();
		let filter = config.filterer.snapshot();

		let roots_changed = self
			.applied
			.as_ref()
			.map_or(true, |applied| applied.pathset != normalized_pathset);
		let filter_changed = self
			.applied
			.as_ref()
			.map_or(true, |applied| !Arc::ptr_eq(&applied.filter, &filter));
		let watcher_changed = self
			.applied
			.as_ref()
			.map_or(true, |applied| applied.watcher != watcher);
		let follow_symlinks_changed = self
			.applied
			.as_ref()
			.map_or(true, |applied| applied.follow_symlinks != follow_symlinks);
		let configuration_changed =
			roots_changed || filter_changed || watcher_changed || follow_symlinks_changed;
		if configuration_changed {
			self.retained_generations.clear();
		}
		let needs_retry = self.recursor.as_ref().map_or(false, Recursor::needs_retry);
		let recreate = self.applied.is_none()
			|| watcher_changed
			|| follow_symlinks_changed
			|| strategy_requires_recreation(strategy, roots_changed, needs_retry);

		self.applied = Some(AppliedConfig {
			pathset: normalized_pathset,
			watcher,
			follow_symlinks,
			filter: filter.clone(),
		});

		if pathset.is_empty() {
			let was_active = self.recursor.take().is_some();
			if was_active {
				trace!("no more watched paths, dropping watcher");
			} else {
				trace!("no watched paths, no watcher needed");
			}
			self.bump_generation();
			return Ok(());
		}

		if recreate || self.recursor.is_none() {
			debug!(kind=?watcher, follow_symlinks, "creating new watcher");
			if !configuration_changed && self.recursor.is_some() {
				// Recreating for a retry does not invalidate events already accepted
				// under the unchanged configuration.
				self.retained_generations.insert(self.generation);
			}
			// Release the old backend before creating the replacement, especially
			// when watcher handles themselves are the exhausted resource.
			self.recursor.take();
			let backend = self.create_backend(watcher, follow_symlinks)?;
			let mut recursor = Recursor::new(
				backend,
				Box::new(FsScanner),
				watcher,
				watcher.backend_kind(),
				follow_symlinks,
				self.cwd.clone(),
				filter.clone(),
			);
			recursor.reconcile(&pathset, filter);
			self.recursor = Some(recursor);
		} else if strategy.owns_logical_tree() && (roots_changed || filter_changed || needs_retry) {
			trace!(
				roots_changed,
				filter_changed,
				"reconciling filesystem sources"
			);
			if let Some(recursor) = self.recursor.as_mut() {
				recursor.reconcile(&pathset, filter);
			}
		}

		Ok(())
	}

	fn request_ready(&mut self, revision: u64) {
		if revision > 0 {
			self.pending_ready = Some(PendingReady {
				topology_fence: None,
			});
		}
	}

	fn capture_ready_fence(&mut self) {
		if !matches!(
			self.pending_ready,
			Some(PendingReady {
				topology_fence: None,
				..
			})
		) {
			return;
		}
		let fence = {
			let _serial = self
				.callback_fence
				.serial
				.lock()
				.unwrap_or_else(std::sync::PoisonError::into_inner);
			self.callback_fence
				.topology_enqueued
				.load(Ordering::Acquire)
		};
		if let Some(ready) = self.pending_ready.as_mut() {
			ready.topology_fence = Some(fence);
		}
	}

	fn bump_generation(&mut self) -> u64 {
		self.generation = self.generation.wrapping_add(1);
		self.callback_fence
			.generation
			.store(self.generation, Ordering::Release);
		self.generation
	}

	fn create_backend(
		&mut self,
		watcher: Watcher,
		follow_symlinks: bool,
	) -> Result<Box<dyn Backend>, CriticalError> {
		let generation = self.bump_generation();
		let control = self.control.clone();
		let ordinary = self.ordinary.clone();
		let callback_fence = self.callback_fence.clone();

		watcher.create(follow_symlinks, move |event| {
			dispatch_callback_event(
				generation,
				watcher,
				&control,
				&ordinary,
				&callback_fence,
				event,
			);
		})
	}

	fn rebuild_backend(
		&mut self,
		runtime_errors: &mut VecDeque<RuntimeError>,
	) -> Result<(), CriticalError> {
		let Some((watcher, follow_symlinks)) = self
			.applied
			.as_ref()
			.map(|applied| (applied.watcher, applied.follow_symlinks))
		else {
			return Ok(());
		};
		let replay_limit = self
			.recursor
			.as_ref()
			.map_or(1, |recursor| recursor.replay_count().saturating_add(2));
		for _ in 0..replay_limit {
			if let Some(recursor) = self.recursor.as_mut() {
				recursor.prepare_backend_rebuild();
			}
			// The backend incarnation changes, but events accepted before this
			// internal repair still belong to the active configuration.
			self.retained_generations.insert(self.generation);
			let backend = self.create_backend(watcher, follow_symlinks)?;
			let Some(recursor) = self.recursor.as_mut() else {
				return Ok(());
			};
			recursor.install_backend(backend);
			let mut replay = recursor.replay_backend_snapshot();
			runtime_errors.extend(replay.errors.drain(..));
			if !replay.rebuild_backend {
				return Ok(());
			}
		}

		// Every replay path gets at most one fresh-backend retry. If backend
		// failures make no progress beyond that finite guard, leave unresolved
		// registrations pending for a later configuration retry rather than
		// exposing asynchronous replay or recursing forever.
		if let Some(recursor) = self.recursor.as_mut() {
			recursor.defer_replay();
		}
		Ok(())
	}

	fn handle_topology(&mut self, change: Topology) {
		let Some(recursor) = self.recursor.as_mut() else {
			return;
		};
		if !recursor.is_managed() {
			return;
		}

		match change {
			Topology::Create(path) => recursor.topology_create(path),
			Topology::Remove(path) => recursor.topology_remove(path),
			Topology::Rename { from, to } => recursor.topology_rename(from, to),
			Topology::Ambiguous(path) => recursor.topology_ambiguous(path),
			Topology::Rescan => recursor.rescan(),
		}
	}

	fn handle_control(&mut self, message: ControlMessage, errors: &mut VecDeque<RuntimeError>) {
		match message {
			ControlMessage::Topology {
				generation,
				sequence,
				changes,
			} => {
				self.processed_topology = self.processed_topology.max(sequence);
				if generation != self.generation {
					trace!("ignoring topology from obsolete filesystem watcher");
					return;
				}
				for change in changes {
					self.handle_topology(change);
				}
			}
			ControlMessage::Error { watcher, error } => {
				// Callback-side generation fencing accepted this error. Once enqueued it
				// remains observable even if reconfiguration wins the worker race.
				if let Err(error) = process_event(Err(error), watcher, &self.events) {
					errors.push_back(error);
				}
			}
		}
	}

	fn recursor_has_work(&self) -> bool {
		self.recursor.as_ref().map_or(false, Recursor::has_work)
	}

	fn control_allowed(&self) -> bool {
		self.pending_ready.map_or(true, |ready| {
			ready
				.topology_fence
				.map_or(true, |fence| self.processed_topology < fence)
		})
	}

	fn ordinary_ready(&self, message: &OrdinaryMessage) -> bool {
		message.topology_sequence <= self.processed_topology && !self.recursor_has_work()
	}

	fn handle_ordinary(&mut self, message: OrdinaryMessage, errors: &mut VecDeque<RuntimeError>) {
		self.retained_generations
			.retain(|generation| *generation >= message.generation);
		if message.generation != self.generation
			&& !self.retained_generations.contains(&message.generation)
		{
			trace!("ignoring event from obsolete filesystem watcher");
			return;
		}

		let public = self.recursor.as_ref().map_or(true, |recursor| {
			!recursor.is_managed() || recursor.event_is_public(&message.event)
		});
		if !public {
			return;
		}
		let watcher = self
			.applied
			.as_ref()
			.map_or_else(Watcher::default, |applied| applied.watcher);
		if let Err(error) = process_event(Ok(message.event), watcher, &self.events) {
			errors.push_back(error);
		}
	}
}

fn deliver_errors_before_critical(
	errors: &mpsc::Sender<RuntimeError>,
	pending: &mut VecDeque<RuntimeError>,
) {
	while let Some(error) = pending.pop_front() {
		match errors.try_send(error) {
			Ok(()) => {}
			Err(
				mpsc::error::TrySendError::Full(error) | mpsc::error::TrySendError::Closed(error),
			) => {
				tracing::error!(
					?error,
					"runtime error preceding critical filesystem failure"
				);
				for error in pending.drain(..) {
					tracing::error!(
						?error,
						"runtime error preceding critical filesystem failure"
					);
				}
				break;
			}
		}
	}
}

/// Launch the filesystem event worker.
///
/// While you can run several, you should only have one.
///
/// This only does a bare minimum of setup; to actually start the work, you need to set a non-empty
/// pathset in the [`Config`].
///
/// Note that the paths emitted by the watcher are normalised. No guarantee is made about the
/// implementation or output of that normalisation (it may change without notice).
///
/// # Examples
///
/// Direct usage:
///
/// ```no_run
/// use async_priority_channel as priority;
/// use tokio::sync::mpsc;
/// use watchexec::{Config, sources::fs::worker};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let (ev_s, _) = priority::bounded(1024);
///     let (er_s, _) = mpsc::channel(64);
///
///     let config = Config::default();
///     config.pathset(["."]);
///
///     worker(config.into(), er_s, ev_s).await?;
///     Ok(())
/// }
/// ```
pub async fn worker(
	config: Arc<Config>,
	errors: mpsc::Sender<RuntimeError>,
	events: priority::Sender<Event, Priority>,
) -> Result<(), CriticalError> {
	debug!("launching filesystem worker");
	let cwd = std::env::current_dir().map_err(|err| CriticalError::IoError {
		about: "obtaining current directory for filesystem watching",
		err,
	})?;
	let (control_tx, mut control_rx) = mpsc::unbounded_channel();
	let (ordinary_tx, mut ordinary_rx) = mpsc::channel(config.event_channel_size.max(1));
	let callback_fence = Arc::new(CallbackFence::default());
	let mut state = WorkerState::new(cwd, control_tx, ordinary_tx, callback_fence, events);
	let mut config_watch = config.watch();
	let mut runtime_errors = VecDeque::new();
	let mut pending_ordinary = None;

	let revision = config_watch.next().await;
	state.apply_config(&config)?;
	state.request_ready(revision);

	loop {
		if config_watch.pending() {
			let revision = config_watch.next().await;
			state.apply_config(&config)?;
			state.request_ready(revision);
			continue;
		}

		if state
			.pending_ready
			.as_ref()
			.map_or(false, |ready| ready.topology_fence.is_none())
			&& !state.recursor_has_work()
		{
			state.capture_ready_fence();
		}

		let mut progressed = false;
		let control_allowed = state.control_allowed();
		if control_allowed {
			match control_rx.try_recv() {
				Ok(message) => {
					state.handle_control(message, &mut runtime_errors);
					progressed = true;
				}
				Err(mpsc::error::TryRecvError::Empty | mpsc::error::TryRecvError::Disconnected) => {
				}
			}
		}

		if state.recursor_has_work() {
			let step = state
				.recursor
				.as_mut()
				.expect("recursor disappeared")
				.step();
			progressed = true;
			runtime_errors.extend(step.errors);
			if step.rebuild_backend {
				// Backend state and finite known-good replay must advance even while
				// the bounded runtime-error channel is backpressured.
				if let Err(critical) = state.rebuild_backend(&mut runtime_errors) {
					deliver_errors_before_critical(&errors, &mut runtime_errors);
					return Err(critical);
				}
			}
		}

		if config_watch.pending() {
			continue;
		}

		if let Some(ready) = state.pending_ready {
			if ready
				.topology_fence
				.map_or(false, |fence| state.processed_topology >= fence)
				&& !state.recursor_has_work()
			{
				state.pending_ready = None;
				let _ = config.fs_ready.send(());
				progressed = true;
			}
		}

		if state.pending_ready.is_none() {
			if let Some(message) = pending_ordinary.take() {
				if state.ordinary_ready(&message) {
					state.handle_ordinary(message, &mut runtime_errors);
					progressed = true;
				} else {
					pending_ordinary = Some(message);
				}
			}
			if pending_ordinary.is_none() {
				match ordinary_rx.try_recv() {
					Ok(message) => {
						pending_ordinary = Some(message);
						progressed = true;
					}
					Err(
						mpsc::error::TryRecvError::Empty | mpsc::error::TryRecvError::Disconnected,
					) => {}
				}
			}
		}

		if let Some(error) = runtime_errors.pop_front() {
			match errors.try_send(error) {
				Ok(()) => progressed = true,
				Err(mpsc::error::TrySendError::Full(error)) => {
					runtime_errors.push_front(error);
				}
				Err(mpsc::error::TrySendError::Closed(error)) => {
					return Err(mpsc::error::SendError(error).into());
				}
			}
		}

		if progressed {
			tokio::task::yield_now().await;
			continue;
		}

		let control_allowed = state.control_allowed();
		tokio::select! {
			biased;
			revision = config_watch.next() => {
				state.apply_config(&config)?;
				state.request_ready(revision);
			}
			Some(message) = control_rx.recv(), if control_allowed => {
				state.handle_control(message, &mut runtime_errors);
			}
			Some(message) = ordinary_rx.recv(), if pending_ordinary.is_none() && state.pending_ready.is_none() => {
				pending_ordinary = Some(message);
			}
			permit = errors.reserve(), if !runtime_errors.is_empty() => {
				if let Ok(permit) = permit {
					permit.send(runtime_errors.pop_front().expect("error queue emptied"));
				} else {
					let error = runtime_errors.pop_front().expect("error queue emptied");
					return Err(mpsc::error::SendError(error).into());
				}
			}
		}
	}
}

fn topology_messages(event: &notify::Event) -> Vec<Topology> {
	let mut changes = Vec::new();
	if event.need_rescan() {
		changes.push(Topology::Rescan);
	}

	match event.kind {
		EventKind::Create(_) => changes.extend(event.paths.iter().cloned().map(Topology::Create)),
		EventKind::Remove(_) => changes.extend(event.paths.iter().cloned().map(Topology::Remove)),
		EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() >= 2 => {
			changes.push(Topology::Rename {
				from: event.paths[0].clone(),
				to: event.paths[1].clone(),
			});
			changes.extend(event.paths.iter().skip(2).cloned().map(Topology::Ambiguous));
		}
		EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
			changes.extend(event.paths.iter().cloned().map(Topology::Remove));
		}
		EventKind::Modify(ModifyKind::Name(_)) | EventKind::Any | EventKind::Other => {
			changes.extend(event.paths.iter().cloned().map(Topology::Ambiguous));
		}
		_ => {}
	}

	changes
}

fn notify_multi_path_errors(
	kind: Watcher,
	watched_path: WatchedPath,
	mut err: notify::Error,
	rm: bool,
) -> Vec<RuntimeError> {
	let mut paths = take(&mut err.paths);
	if paths.is_empty() {
		paths.push(watched_path.into());
	}

	let generic = err.to_string();
	let mut err = Some(err);

	let mut errs = Vec::with_capacity(paths.len());
	for path in paths {
		let error = err
			.take()
			.unwrap_or_else(|| notify::Error::generic(&generic))
			.add_path(path.clone());

		errs.push(RuntimeError::FsWatcher {
			kind,
			err: if rm {
				FsWatcherError::PathRemove { path, err: error }
			} else {
				FsWatcherError::PathAdd { path, err: error }
			},
		});
	}

	errs
}

fn process_event(
	nev: Result<notify::Event, notify::Error>,
	kind: Watcher,
	n_events: &priority::Sender<Event, Priority>,
) -> Result<(), RuntimeError> {
	let nev = nev.map_err(|err| RuntimeError::FsWatcher {
		kind,
		err: FsWatcherError::Event(err),
	})?;

	let event = event_from_notify(nev);

	trace!(?event, "processed notify event into watchexec event");
	match n_events.try_send(event, Priority::Normal) {
		Ok(()) => {}
		Err(priority::TrySendError::Full(_)) => {
			debug!(
				"fs watcher event channel is full; dropping event \
				 (tune Config::event_channel_size if this happens often)"
			);
		}
		Err(priority::TrySendError::Closed(event)) => {
			return Err(RuntimeError::EventChannelSend {
				ctx: "fs watcher",
				err: priority::SendError(event),
			});
		}
	}

	Ok(())
}

fn event_from_notify(nev: notify::Event) -> Event {
	let mut tags = Vec::with_capacity(4);
	tags.push(Tag::Source(Source::Filesystem));
	tags.push(Tag::FileEventKind(nev.kind));

	for path in nev.paths {
		tags.push(Tag::Path {
			file_type: metadata(&path)
				.ok()
				.map(|metadata| metadata.file_type().into()),
			path: path.normalize(),
		});
	}

	if let Some(pid) = nev.attrs.process_id() {
		tags.push(Tag::Process(pid));
	}

	let mut event_metadata = HashMap::new();

	if let Some(uid) = nev.attrs.info() {
		event_metadata.insert("file-event-info".to_string(), vec![uid.to_string()]);
	}

	if let Some(src) = nev.attrs.source() {
		event_metadata.insert("notify-backend".to_string(), vec![src.to_string()]);
	}

	Event {
		tags,
		metadata: event_metadata,
	}
}

#[cfg(test)]
mod tests {
	use super::{
		dispatch_callback_event, process_event, recursion_strategy, strategy_requires_recreation,
		topology_messages, worker, CallbackFence, ControlMessage, OrdinaryMessage,
		RecursionStrategy, Topology, Watcher, WorkerState,
	};
	use crate::{
		error::{FsWatcherError, RuntimeError},
		Config,
	};
	use async_priority_channel as priority;
	use futures::FutureExt as _;
	use notify::{
		event::{Flag, ModifyKind, RenameMode},
		EventKind,
	};
	use std::{
		collections::VecDeque,
		path::PathBuf,
		sync::{atomic::Ordering, Arc},
	};
	use tokio::sync::mpsc;
	use watchexec_events::Priority;

	// Regression test for issue #920: when the bounded event channel is full,
	// `process_event` used to propagate `RuntimeError::EventChannelTrySend`,
	// surfacing "cannot send event from fs watcher: sending into a full channel"
	// to the user as a non-fatal error for every dropped event. It should instead
	// drop the event gracefully and return `Ok(())`.
	#[test]
	fn backend_strategy_matches_notify_capabilities() {
		assert_eq!(
			Watcher::Poll(std::time::Duration::from_secs(1)).recursion_strategy(),
			RecursionStrategy::PerDirectory
		);
		assert_eq!(
			recursion_strategy(notify::WatcherKind::Inotify),
			RecursionStrategy::PerDirectory
		);
		assert_eq!(
			recursion_strategy(notify::WatcherKind::ReadDirectoryChangesWatcher),
			RecursionStrategy::PerDirectory
		);
		assert_eq!(
			recursion_strategy(notify::WatcherKind::PollWatcher),
			RecursionStrategy::PerDirectory
		);
		assert_eq!(
			recursion_strategy(notify::WatcherKind::Fsevent),
			RecursionStrategy::RecursiveRoots
		);
		assert_eq!(
			recursion_strategy(notify::WatcherKind::Kqueue),
			RecursionStrategy::NotifyOwned
		);
		assert_eq!(
			recursion_strategy(notify::WatcherKind::NullWatcher),
			RecursionStrategy::NotifyOwned
		);
		let recommended = <notify::RecommendedWatcher as notify::Watcher>::kind();
		let native_kind = Watcher::Native.backend_kind_for(recommended);
		assert_eq!(Watcher::Native.backend_kind(), native_kind);
		assert_eq!(
			Watcher::Native.recursion_strategy(),
			recursion_strategy(native_kind)
		);
	}

	#[test]
	fn recursive_root_strategy_recreates_for_root_changes_or_retry() {
		assert!(!strategy_requires_recreation(
			RecursionStrategy::RecursiveRoots,
			false,
			false
		));
		assert!(strategy_requires_recreation(
			RecursionStrategy::RecursiveRoots,
			false,
			true
		));
		assert!(strategy_requires_recreation(
			RecursionStrategy::RecursiveRoots,
			true,
			false
		));
	}

	#[test]
	fn native_backend_falls_back_from_kqueue_to_poll() {
		assert_eq!(
			Watcher::Native.backend_kind_for(notify::WatcherKind::Kqueue),
			notify::WatcherKind::PollWatcher
		);
	}

	#[test]
	fn native_backend_preserves_other_recommendations() {
		for kind in [
			notify::WatcherKind::Inotify,
			notify::WatcherKind::Fsevent,
			notify::WatcherKind::PollWatcher,
			notify::WatcherKind::ReadDirectoryChangesWatcher,
			notify::WatcherKind::NullWatcher,
		] {
			assert_eq!(Watcher::Native.backend_kind_for(kind), kind);
		}
	}

	#[test]
	fn explicit_poll_ignores_native_recommendation() {
		let poll = Watcher::Poll(std::time::Duration::from_secs(1));
		for recommended in [
			notify::WatcherKind::Inotify,
			notify::WatcherKind::Fsevent,
			notify::WatcherKind::Kqueue,
			notify::WatcherKind::ReadDirectoryChangesWatcher,
			notify::WatcherKind::NullWatcher,
		] {
			assert_eq!(
				poll.backend_kind_for(recommended),
				notify::WatcherKind::PollWatcher
			);
		}
	}

	#[test]
	fn process_event_drops_when_channel_full() {
		let (ev_s, _ev_r) = priority::bounded::<watchexec_events::Event, Priority>(1);
		let nev = Ok(notify::Event::new(EventKind::Any));

		assert!(process_event(nev, super::Watcher::default(), &ev_s).is_ok());

		let nev = Ok(notify::Event::new(EventKind::Any));
		let res = process_event(nev, super::Watcher::default(), &ev_s);
		assert!(
			res.is_ok(),
			"full channel should drop the event silently, not return a RuntimeError (got {res:?})",
		);
	}

	#[test]
	fn process_event_propagates_when_channel_closed() {
		let (ev_s, ev_r) = priority::bounded::<watchexec_events::Event, Priority>(1);
		drop(ev_r);

		let nev = Ok(notify::Event::new(EventKind::Any));
		let res = process_event(nev, super::Watcher::default(), &ev_s);
		assert!(
			matches!(res, Err(RuntimeError::EventChannelSend { .. })),
			"closed channel should propagate as EventChannelSend, got {res:?}",
		);
	}

	#[test]
	fn topology_lane_remains_lossless_when_ordinary_lane_is_full() {
		let (control, mut control_rx) = mpsc::unbounded_channel();
		let (ordinary, mut ordinary_rx) = mpsc::channel(1);
		let fence = CallbackFence::default();
		fence.generation.store(1, Ordering::Release);

		dispatch_callback_event(
			1,
			Watcher::Poll(std::time::Duration::from_secs(1)),
			&control,
			&ordinary,
			&fence,
			Ok(notify::Event::new(EventKind::Access(
				notify::event::AccessKind::Any,
			))),
		);
		dispatch_callback_event(
			1,
			Watcher::Poll(std::time::Duration::from_secs(1)),
			&control,
			&ordinary,
			&fence,
			Ok(
				notify::Event::new(EventKind::Create(notify::event::CreateKind::Folder))
					.add_path("/root/new".into()),
			),
		);

		assert!(ordinary_rx.try_recv().is_ok());
		assert!(matches!(
			ordinary_rx.try_recv(),
			Err(mpsc::error::TryRecvError::Empty)
		));
		assert!(matches!(
			control_rx.try_recv(),
			Ok(ControlMessage::Topology {
				generation: 1,
				sequence: 1,
				changes,
			}) if changes == vec![Topology::Create(PathBuf::from("/root/new"))]
		));
	}

	#[test]
	fn callback_rejects_obsolete_generation_before_enqueue() {
		let (control, mut control_rx) = mpsc::unbounded_channel();
		let (ordinary, mut ordinary_rx) = mpsc::channel(1);
		let fence = CallbackFence::default();
		fence.generation.store(2, Ordering::Release);

		dispatch_callback_event(
			1,
			Watcher::Native,
			&control,
			&ordinary,
			&fence,
			Ok(
				notify::Event::new(EventKind::Create(notify::event::CreateKind::Folder))
					.add_path("/obsolete".into()),
			),
		);

		assert!(matches!(
			control_rx.try_recv(),
			Err(mpsc::error::TryRecvError::Empty)
		));
		assert!(matches!(
			ordinary_rx.try_recv(),
			Err(mpsc::error::TryRecvError::Empty)
		));
	}

	#[test]
	fn accepted_callback_error_survives_reconfiguration_race() {
		let (control, mut control_rx) = mpsc::unbounded_channel();
		let (ordinary, _ordinary_rx) = mpsc::channel(1);
		let (events, _event_rx) = priority::bounded(1);
		let callback_fence = Arc::new(CallbackFence::default());
		callback_fence.generation.store(1, Ordering::Release);
		let original = Watcher::Poll(std::time::Duration::from_secs(7));
		dispatch_callback_event(
			1,
			original,
			&control,
			&ordinary,
			&callback_fence,
			Err(notify::Error::generic("accepted callback error")),
		);

		let mut state = WorkerState::new(
			PathBuf::from("/work"),
			control,
			ordinary,
			callback_fence,
			events,
		);
		state.generation = 2;
		let mut errors = VecDeque::new();
		state.handle_control(control_rx.try_recv().unwrap(), &mut errors);

		assert!(matches!(
			errors.pop_front(),
			Some(RuntimeError::FsWatcher {
				kind,
				err: FsWatcherError::Event(_),
			}) if kind == original
		));
	}

	#[test]
	fn readiness_captures_topology_only_at_first_quiescence() {
		let (control, _control_rx) = mpsc::unbounded_channel();
		let (ordinary, _ordinary_rx) = mpsc::channel(1);
		let (events, _event_rx) = priority::bounded(1);
		let callback_fence = Arc::new(CallbackFence::default());
		callback_fence.topology_enqueued.store(3, Ordering::Release);
		let mut state = WorkerState::new(
			PathBuf::from("/work"),
			control,
			ordinary,
			callback_fence.clone(),
			events,
		);

		state.request_ready(1);
		assert_eq!(state.pending_ready.unwrap().topology_fence, None);
		callback_fence.topology_enqueued.store(4, Ordering::Release);
		state.capture_ready_fence();

		assert_eq!(state.pending_ready.unwrap().topology_fence, Some(4));
	}

	#[test]
	fn readiness_fence_waits_for_callback_holding_enqueue_gate() {
		let (control, _control_rx) = mpsc::unbounded_channel();
		let (ordinary, _ordinary_rx) = mpsc::channel(1);
		let (events, _event_rx) = priority::bounded(1);
		let callback_fence = Arc::new(CallbackFence::default());
		let mut state = WorkerState::new(
			PathBuf::from("/work"),
			control.clone(),
			ordinary,
			callback_fence.clone(),
			events,
		);
		state.request_ready(1);
		let (acquired_tx, acquired_rx) = std::sync::mpsc::channel();
		let (release_tx, release_rx) = std::sync::mpsc::channel();

		std::thread::scope(|scope| {
			let gate = callback_fence.clone();
			scope.spawn(move || {
				let _serial = gate.serial.lock().unwrap();
				acquired_tx.send(()).unwrap();
				release_rx.recv().unwrap();
				let sequence = gate.topology_enqueued.fetch_add(1, Ordering::AcqRel) + 1;
				control
					.send(ControlMessage::Topology {
						generation: 0,
						sequence,
						changes: vec![Topology::Create("/during-reconcile".into())],
					})
					.unwrap();
			});
			acquired_rx.recv().unwrap();
			release_tx.send(()).unwrap();
			state.capture_ready_fence();
		});

		assert_eq!(state.pending_ready.unwrap().topology_fence, Some(1));
	}

	#[test]
	fn paired_rename_preserves_from_to_order() {
		let event = notify::Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
			.add_path(PathBuf::from("from"))
			.add_path(PathBuf::from("to"));
		assert_eq!(
			topology_messages(&event),
			vec![Topology::Rename {
				from: PathBuf::from("from"),
				to: PathBuf::from("to"),
			}]
		);
	}

	#[test]
	fn obsolete_generation_does_not_emit_ordinary_event() {
		let (control, _control_rx) = mpsc::unbounded_channel();
		let (ordinary, _ordinary_rx) = mpsc::channel(4);
		let (events, event_rx) = priority::bounded(4);
		let mut state = WorkerState::new(
			PathBuf::from("/work"),
			control,
			ordinary,
			Arc::new(CallbackFence::default()),
			events,
		);
		state.generation = 2;
		let mut errors = VecDeque::new();

		state.handle_ordinary(
			OrdinaryMessage {
				generation: 1,
				topology_sequence: 0,
				event: notify::Event::new(EventKind::Any),
			},
			&mut errors,
		);
		assert!(errors.is_empty());
		assert!(matches!(
			event_rx.try_recv(),
			Err(priority::TryRecvError::Empty)
		));
	}

	#[test]
	fn internally_rebuilt_generation_emits_accepted_ordinary_event() {
		let (control, _control_rx) = mpsc::unbounded_channel();
		let (ordinary, _ordinary_rx) = mpsc::channel(4);
		let (events, event_rx) = priority::bounded(4);
		let mut state = WorkerState::new(
			PathBuf::from("/work"),
			control,
			ordinary,
			Arc::new(CallbackFence::default()),
			events,
		);
		state.generation = 2;
		state.retained_generations.insert(1);
		let mut errors = VecDeque::new();

		state.handle_ordinary(
			OrdinaryMessage {
				generation: 1,
				topology_sequence: 1,
				event: notify::Event::new(EventKind::Any).add_path("/retargeted".into()),
			},
			&mut errors,
		);

		assert!(errors.is_empty());
		assert!(event_rx.try_recv().is_ok());
	}

	#[test]
	fn current_generation_closed_event_error_remains_reliable() {
		let (control, _control_rx) = mpsc::unbounded_channel();
		let (ordinary, _ordinary_rx) = mpsc::channel(4);
		let (events, event_rx) = priority::bounded(4);
		drop(event_rx);
		let mut state = WorkerState::new(
			PathBuf::from("/work"),
			control,
			ordinary,
			Arc::new(CallbackFence::default()),
			events,
		);
		state.generation = 1;
		let mut errors = VecDeque::new();

		state.handle_control(
			ControlMessage::Topology {
				generation: 1,
				sequence: 1,
				changes: vec![Topology::Rescan],
			},
			&mut errors,
		);
		state.handle_ordinary(
			OrdinaryMessage {
				generation: 1,
				topology_sequence: 1,
				event: notify::Event::new(EventKind::Any),
			},
			&mut errors,
		);
		assert!(matches!(
			errors.pop_front(),
			Some(RuntimeError::EventChannelSend { .. })
		));
	}

	#[test]
	fn worker_skips_revision_zero_ready_but_signals_same_value_revision() {
		let runtime = tokio::runtime::Builder::new_current_thread()
			.build()
			.unwrap();
		runtime.block_on(async {
			let config = Arc::new(Config::default());
			let mut ready = config.fs_ready();
			let (events, _event_rx) = priority::bounded(4);
			let (errors, _error_rx) = mpsc::channel(4);
			let task = tokio::spawn(worker(config.clone(), errors, events));

			for _ in 0..4 {
				tokio::task::yield_now().await;
			}
			assert!(ready.changed().now_or_never().is_none());

			config.pathset(Vec::<super::WatchedPath>::new());
			ready.changed().await.unwrap();
			task.abort();
		});
	}

	#[test]
	fn split_rename_and_rescan_produce_reliable_topology() {
		let from = notify::Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::From)))
			.add_path(PathBuf::from("old"));
		let to = notify::Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::To)))
			.add_path(PathBuf::from("new"));
		let rescan = notify::Event::new(EventKind::Any).set_flag(Flag::Rescan);

		assert_eq!(
			topology_messages(&from),
			vec![Topology::Remove(PathBuf::from("old"))]
		);
		assert_eq!(
			topology_messages(&to),
			vec![Topology::Ambiguous(PathBuf::from("new"))]
		);
		assert_eq!(topology_messages(&rescan), vec![Topology::Rescan]);
	}
}
