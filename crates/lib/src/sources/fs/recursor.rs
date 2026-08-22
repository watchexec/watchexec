use std::{
	cmp::Reverse,
	collections::{HashMap, HashSet, VecDeque},
	fs, io,
	path::{Path, PathBuf},
	sync::Arc,
};

use normalize_path::NormalizePath;
use notify::RecursiveMode;

use crate::{
	error::{FsWatcherError, RuntimeError},
	filter::Filterer,
	WatchedPath,
};

use super::{
	backend::{classify_resource_error, Backend, DisconnectedBackend},
	managed_backend, notify_multi_path_errors, Watcher,
};

/// One-path-at-a-time filesystem seam. In particular, `scan` reads only one
/// directory; the recursor owns the breadth-first work queue.
pub(super) trait Scanner: Send {
	fn classify(&self, path: &Path, follow_symlinks: bool) -> io::Result<EntryKind>;
	fn scan(&self, path: &Path, visit: &mut dyn FnMut(io::Result<PathBuf>)) -> io::Result<()>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum EntryKind {
	Directory(PathBuf),
	NonFollowedSymlink,
	Other,
}

pub(super) struct FsScanner;

impl Scanner for FsScanner {
	fn classify(&self, path: &Path, follow_symlinks: bool) -> io::Result<EntryKind> {
		let link_metadata = fs::symlink_metadata(path)?;
		if link_metadata.file_type().is_symlink() && !follow_symlinks {
			return Ok(EntryKind::NonFollowedSymlink);
		}

		let metadata = if link_metadata.file_type().is_symlink() {
			fs::metadata(path)?
		} else {
			link_metadata
		};

		if metadata.is_dir() {
			Ok(EntryKind::Directory(fs::canonicalize(path)?))
		} else {
			Ok(EntryKind::Other)
		}
	}

	fn scan(&self, path: &Path, visit: &mut dyn FnMut(io::Result<PathBuf>)) -> io::Result<()> {
		for entry in fs::read_dir(path)? {
			visit(entry.map(|entry| entry.path()));
		}
		Ok(())
	}
}

type Root = WatchedPath;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Identity {
	Canonical(PathBuf),
	Lexical(PathBuf),
}

#[derive(Debug)]
struct LogicalWatch {
	identity: Identity,
	owners: HashMap<Root, u64>,
}

#[derive(Debug)]
struct PhysicalWatch {
	watch_path: PathBuf,
	logicals: HashSet<PathBuf>,
	guards: HashSet<PathBuf>,
	mode: RecursiveMode,
}

#[derive(Debug)]
struct ReplayWatch {
	watch_path: PathBuf,
	mode: RecursiveMode,
	logicals: Vec<(PathBuf, LogicalWatch)>,
}

#[derive(Debug)]
struct GuardWatch {
	identity: Identity,
	owners: HashSet<Root>,
}

#[derive(Debug)]
enum Work {
	Guard(Root),
	Root(Root),
	Candidate {
		root: Root,
		path: PathBuf,
		transient_retries: u8,
	},
	Scan {
		root: Root,
		path: PathBuf,
		transient_retries: u8,
	},
	Probe(PathBuf),
}

impl Work {
	fn path(&self) -> &Path {
		match self {
			Self::Guard(root) => root.path.parent().unwrap_or(&root.path),
			Self::Root(root) => &root.path,
			Self::Candidate { path, .. } | Self::Scan { path, .. } | Self::Probe(path) => path,
		}
	}
}

#[derive(Debug)]
struct SweepEntry {
	path: PathBuf,
	root: Root,
	epoch: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
	Traversing,
	Sweeping,
	Settled,
}

#[derive(Default)]
pub(super) struct StepResult {
	pub(super) errors: Vec<RuntimeError>,
	pub(super) rebuild_backend: bool,
}

const TRANSIENT_RETRIES: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AddResult {
	Added,
	Skipped,
	Invalidated,
	Rebuild,
}

enum ExplicitRootState {
	Entry(EntryKind),
	Missing { path: PathBuf, error: io::Error },
	Unsafe,
}

const fn path_not_found(error: &notify::Error) -> bool {
	matches!(error.kind, notify::ErrorKind::PathNotFound)
}

fn mode_satisfies(actual: RecursiveMode, requested: RecursiveMode) -> bool {
	actual == requested
		|| matches!(
			(actual, requested),
			(RecursiveMode::Recursive, RecursiveMode::NonRecursive)
		)
}

/// Watchexec-owned recursive registration state.
///
/// Logical paths retain per-root ownership while physical registrations are
/// shared by canonical identity. All backend mutations happen from `step`.
pub(super) struct Recursor {
	backend: Box<dyn Backend>,
	scanner: Box<dyn Scanner>,
	kind: Watcher,
	backend_kind: notify::WatcherKind,
	follow_symlinks: bool,
	cwd: PathBuf,
	filter: Arc<dyn Filterer>,
	roots: HashSet<Root>,
	epoch: u64,
	logical: HashMap<PathBuf, LogicalWatch>,
	physical: HashMap<Identity, PhysicalWatch>,
	replay_desired: HashMap<Identity, ReplayWatch>,
	replay_queue: VecDeque<Identity>,
	rebuild_exclusions: HashSet<PathBuf>,
	guards: HashMap<PathBuf, GuardWatch>,
	root_guards: HashMap<Root, PathBuf>,
	seen: HashMap<Root, HashSet<Identity>>,
	work: VecDeque<Work>,
	pending_removals: VecDeque<PathBuf>,
	pending_removal_set: HashSet<PathBuf>,
	pending_owner_removals: VecDeque<(PathBuf, Root)>,
	sweep: VecDeque<SweepEntry>,
	phase: Phase,
	rebuild_requested: bool,
	resource_latched: bool,
	resource_reported: bool,
	skipped_additions: HashSet<PathBuf>,
	addition_failures: HashMap<PathBuf, u8>,
	retry_candidates: HashSet<(Root, PathBuf)>,
	retry_roots: HashSet<Root>,
	removed_prefixes: HashSet<PathBuf>,
	refresh_tombstones: bool,
}

impl Recursor {
	pub(super) fn new(
		backend: Box<dyn Backend>,
		scanner: Box<dyn Scanner>,
		kind: Watcher,
		backend_kind: notify::WatcherKind,
		follow_symlinks: bool,
		cwd: PathBuf,
		filter: Arc<dyn Filterer>,
	) -> Self {
		Self {
			backend,
			scanner,
			kind,
			backend_kind,
			follow_symlinks,
			cwd,
			filter,
			roots: HashSet::new(),
			epoch: 0,
			logical: HashMap::new(),
			physical: HashMap::new(),
			replay_desired: HashMap::new(),
			replay_queue: VecDeque::new(),
			rebuild_exclusions: HashSet::new(),
			guards: HashMap::new(),
			root_guards: HashMap::new(),
			seen: HashMap::new(),
			work: VecDeque::new(),
			pending_removals: VecDeque::new(),
			pending_removal_set: HashSet::new(),
			pending_owner_removals: VecDeque::new(),
			sweep: VecDeque::new(),
			phase: Phase::Settled,
			rebuild_requested: false,
			resource_latched: false,
			resource_reported: false,
			skipped_additions: HashSet::new(),
			addition_failures: HashMap::new(),
			retry_candidates: HashSet::new(),
			retry_roots: HashSet::new(),
			removed_prefixes: HashSet::new(),
			refresh_tombstones: false,
		}
	}

	pub(super) const fn is_managed(&self) -> bool {
		managed_backend(self.backend_kind)
	}

	pub(super) fn needs_retry(&self) -> bool {
		!self.retry_roots.is_empty()
			|| !self.retry_candidates.is_empty()
			|| !self.replay_desired.is_empty()
			|| self.resource_latched
	}

	pub(super) fn event_is_public(&self, event: &notify::Event) -> bool {
		event.paths.is_empty()
			|| event.paths.iter().any(|path| {
				let path = self.absolute(path);
				self.roots.iter().any(|root| {
					path == root.path
						|| (root.recursive && path.starts_with(&root.path))
						|| (!root.recursive && path.parent() == Some(root.path.as_path()))
				})
			})
	}

	pub(super) fn has_work(&self) -> bool {
		self.rebuild_requested
			|| !self.replay_queue.is_empty()
			|| !self.pending_removals.is_empty()
			|| !self.pending_owner_removals.is_empty()
			|| !self.work.is_empty()
			|| !self.sweep.is_empty()
			|| self.phase != Phase::Settled
	}

	pub(super) fn reconcile(&mut self, pathset: &[WatchedPath], filter: Arc<dyn Filterer>) {
		self.filter = filter;
		self.roots = pathset
			.iter()
			.map(|path| Root {
				path: self.absolute(path.path.as_ref()),
				recursive: path.recursive,
			})
			.collect();
		self.begin_epoch();
	}

	pub(super) fn rescan(&mut self) {
		self.begin_epoch();
		if self.is_managed() {
			self.rebuild_requested = true;
		}
	}

	pub(super) fn prepare_backend_rebuild(&mut self) {
		// Preserve every registration which was known-good before the uncertain
		// mutation. Replay is performed directly on the fresh backend before a
		// scanner failure can affect that coverage.
		let mut desired = std::mem::take(&mut self.replay_desired);
		let mut logical = std::mem::take(&mut self.logical);
		for (identity, physical) in &self.physical {
			let mut logicals: Vec<_> = physical
				.logicals
				.iter()
				.filter(|path| !self.rebuild_exclusions.contains(*path))
				.filter(|path| {
					!self
						.removed_prefixes
						.iter()
						.any(|prefix| path.starts_with(prefix))
				})
				.filter_map(|path| logical.remove(path).map(|watch| (path.clone(), watch)))
				.collect();
			if logicals.is_empty() {
				continue;
			}
			logicals.sort_by(|(left, _), (right, _)| left.cmp(right));
			let watch_path = if logicals
				.iter()
				.any(|(path, _)| path == &physical.watch_path)
			{
				physical.watch_path.clone()
			} else {
				logicals[0].0.clone()
			};
			desired.insert(
				identity.clone(),
				ReplayWatch {
					watch_path,
					mode: physical.mode,
					logicals,
				},
			);
		}
		self.rebuild_exclusions.clear();

		// Drop the uncertain watcher before trying to create its replacement. This
		// also releases inotify handles/watches which may be the scarce resource.
		self.backend = Box::new(DisconnectedBackend);
		self.physical.clear();
		self.sweep.clear();
		self.resource_latched = false;
		self.replay_desired = desired;
		self.replay_queue.clear();
		self.requeue_replay();
		self.restart_epoch_work();
	}

	pub(super) fn install_backend(&mut self, backend: Box<dyn Backend>) {
		self.backend = backend;
	}

	pub(super) fn replay_count(&self) -> usize {
		self.replay_desired
			.len()
			.saturating_add(self.physical.len())
			.saturating_add(self.guards.len())
	}

	pub(super) fn defer_replay(&mut self) {
		self.replay_queue.clear();
	}

	pub(super) fn replay_backend_snapshot(&mut self) -> StepResult {
		let mut combined = StepResult::default();
		let budget = self.replay_queue.len();
		for _ in 0..budget {
			let Some(identity) = self.replay_queue.pop_front() else {
				break;
			};
			let mut step = StepResult::default();
			self.step_replay(identity, &mut step);
			combined.errors.append(&mut step.errors);
			combined.rebuild_backend |= step.rebuild_backend;
			if combined.rebuild_backend || self.resource_latched {
				return combined;
			}
		}

		let guard_roots: Vec<_> = self.root_guards.keys().cloned().collect();
		for root in guard_roots {
			let mut step = StepResult::default();
			self.ensure_guard(root, &mut step);
			combined.errors.append(&mut step.errors);
			combined.rebuild_backend |= step.rebuild_backend;
			if combined.rebuild_backend || self.resource_latched {
				break;
			}
		}
		combined
	}

	#[cfg(test)]
	fn replace_backend(&mut self, backend: Box<dyn Backend>) {
		self.prepare_backend_rebuild();
		self.install_backend(backend);
	}

	pub(super) fn topology_create(&mut self, path: PathBuf) {
		if !self.is_managed() {
			return;
		}
		let path = self.absolute(&path);
		for path in self.projected_topology_paths(&path) {
			self.topology_create_one(path);
		}
	}

	fn topology_create_one(&mut self, path: PathBuf) {
		self.topology_create_one_with_removal(path, true);
	}

	fn topology_create_one_with_removal(&mut self, path: PathBuf, queue_removal: bool) {
		self.clear_authoritative_prefix(&path);
		let mut roots = self.candidate_roots(&path, false);
		if queue_removal {
			self.queue_remove_prefix(&path);
		}
		self.requeue_guarded_roots_below(&path);
		self.requeue_configured_roots_below(&path);
		for root in roots.drain() {
			self.seen.remove(&root);
			self.retry_candidates.insert((root.clone(), path.clone()));
			self.work.push_front(Work::Candidate {
				root,
				path: path.clone(),
				transient_retries: TRANSIENT_RETRIES,
			});
		}
	}

	pub(super) fn topology_remove(&mut self, path: PathBuf) {
		if !self.is_managed() {
			return;
		}
		let path = self.absolute(&path);
		for path in self.projected_topology_paths(&path) {
			self.topology_remove_one(path);
		}
	}

	fn topology_remove_one(&mut self, path: PathBuf) {
		self.refresh_tombstones = false;
		self.removed_prefixes.insert(path.clone());
		let mut affected: HashSet<_> = self
			.root_guards
			.iter()
			.filter(|(root, guard)| root.path.starts_with(&path) || guard.starts_with(&path))
			.map(|(root, _)| root.clone())
			.collect();
		affected.extend(
			self.roots
				.iter()
				.filter(|root| root.path.starts_with(&path))
				.cloned(),
		);
		self.queue_remove_prefix(&path);
		for root in affected {
			self.work.push_back(Work::Guard(root));
		}
	}

	pub(super) fn topology_rename(&mut self, from: PathBuf, to: PathBuf) {
		if !self.is_managed() {
			return;
		}
		let from = self.absolute(&from);
		let to = self.absolute(&to);
		let from = self.projected_topology_paths(&from);
		let to = self.projected_topology_paths(&to);
		for path in from {
			self.topology_remove_one(path);
		}
		for path in &to {
			self.topology_remove_one(path.clone());
		}
		for path in to {
			self.topology_create_one_with_removal(path, false);
		}
	}

	pub(super) fn topology_ambiguous(&mut self, path: PathBuf) {
		if !self.is_managed() {
			return;
		}
		let path = self.absolute(&path);
		for path in self.projected_topology_paths(&path) {
			self.topology_ambiguous_one(path);
		}
	}

	fn topology_ambiguous_one(&mut self, path: PathBuf) {
		self.clear_authoritative_prefix(&path);
		self.queue_remove_prefix(&path);
		self.work.push_back(Work::Probe(path.clone()));
		self.requeue_guarded_roots_below(&path);
		self.requeue_configured_roots_below(&path);
	}

	fn projected_topology_paths(&self, path: &Path) -> Vec<PathBuf> {
		let mut projected = HashSet::from([path.to_owned()]);
		let Some(parent) = path.parent() else {
			return projected.into_iter().collect();
		};
		let Some(name) = path.file_name() else {
			return projected.into_iter().collect();
		};
		let identities: HashSet<_> = self
			.logical
			.get(parent)
			.map(|watch| &watch.identity)
			.into_iter()
			.chain(self.guards.get(parent).map(|watch| &watch.identity))
			.cloned()
			.collect();
		for identity in identities {
			let Some(physical) = self
				.physical
				.get(&identity)
				.filter(|physical| physical.watch_path == parent)
			else {
				continue;
			};
			for alias in physical.logicals.iter().chain(&physical.guards) {
				projected.insert(alias.join(name));
			}
		}
		let mut projected: Vec<_> = projected.into_iter().collect();
		projected.sort();
		projected
	}

	fn clear_authoritative_prefix(&mut self, path: &Path) {
		self.removed_prefixes
			.retain(|prefix| !prefix.starts_with(path) && !path.starts_with(prefix));
		self.skipped_additions
			.retain(|candidate| !candidate.starts_with(path));
		self.addition_failures
			.retain(|candidate, _| !candidate.starts_with(path));
	}

	fn requeue_guarded_roots_below(&mut self, path: &Path) {
		let guarded: Vec<_> = self
			.root_guards
			.keys()
			.filter(|root| root.path.starts_with(path))
			.cloned()
			.collect();
		for root in guarded {
			// Run before a probe can prune queued work below the newly-created
			// ancestor. This moves the guard down one or more levels immediately.
			self.work.push_front(Work::Guard(root));
		}
	}

	fn requeue_configured_roots_below(&mut self, path: &Path) {
		let roots: Vec<_> = self
			.roots
			.iter()
			.filter(|root| root.path.starts_with(path))
			.cloned()
			.collect();
		for root in roots {
			self.seen.remove(&root);
			let already_queued = self.work.iter().any(
				|work| matches!(work, Work::Root(queued) | Work::Guard(queued) if queued == &root),
			);
			if !already_queued {
				self.work.push_back(Work::Root(root));
			}
		}
	}

	pub(super) fn step(&mut self) -> StepResult {
		let mut result = StepResult::default();

		if self.rebuild_requested {
			self.rebuild_requested = false;
			result.rebuild_backend = true;
			return result;
		}

		if let Some(identity) = self.replay_queue.pop_front() {
			self.step_replay(identity, &mut result);
			return result;
		}

		if let Some(path) = self.pending_removals.pop_front() {
			self.pending_removal_set.remove(&path);
			self.remove_logical(&path, &mut result);
			return result;
		}

		if let Some((path, root)) = self.pending_owner_removals.pop_front() {
			let entry = SweepEntry {
				path,
				root,
				epoch: self.epoch,
			};
			self.remove_owner_force(entry, &mut result);
			return result;
		}

		if self.refresh_tombstones {
			self.removed_prefixes.clear();
			self.refresh_tombstones = false;
		}

		if let Some(work) = self.work.pop_front() {
			self.step_work(work, &mut result);
			return result;
		}

		match self.phase {
			Phase::Traversing => {
				self.prepare_sweep();
				if self.sweep.is_empty() {
					self.settle();
				}
			}
			Phase::Sweeping => {
				if let Some(entry) = self.sweep.pop_front() {
					self.remove_owner(entry, &mut result);
				} else {
					self.settle();
				}
			}
			Phase::Settled => {}
		}

		result
	}

	fn begin_epoch(&mut self) {
		self.epoch = self
			.epoch
			.checked_add(1)
			.expect("filesystem recursion epoch overflow");
		self.resource_latched = false;
		self.resource_reported = false;
		self.skipped_additions.clear();
		self.addition_failures.clear();
		self.rebuild_exclusions.clear();
		self.refresh_tombstones = true;
		self.restart_epoch_work();
	}

	fn restart_epoch_work(&mut self) {
		self.work.clear();
		self.seen.clear();
		self.sweep.clear();
		self.phase = Phase::Traversing;
		self.retry_roots.retain(|root| self.roots.contains(root));
		self.retry_candidates
			.retain(|(root, _)| self.roots.contains(root));
		self.requeue_replay();
		let mut roots: Vec<_> = self.roots.iter().cloned().collect();
		roots.sort_by(|left, right| {
			left.path
				.cmp(&right.path)
				.then_with(|| right.recursive.cmp(&left.recursive))
		});
		if self.is_managed() {
			let mut guarded: Vec<_> = self.root_guards.keys().cloned().collect();
			guarded.sort_by(|left, right| left.path.cmp(&right.path));
			self.work.extend(guarded.into_iter().map(Work::Guard));
			self.work.extend(
				self.retry_candidates
					.iter()
					.filter(|(root, _)| self.roots.contains(root))
					.cloned()
					.map(|(root, path)| Work::Candidate {
						root,
						path,
						transient_retries: TRANSIENT_RETRIES,
					}),
			);
		}
		self.work.extend(
			self.retry_roots
				.iter()
				.filter(|root| self.roots.contains(*root))
				.cloned()
				.map(Work::Root),
		);
		self.work.extend(roots.into_iter().map(Work::Root));
	}

	fn requeue_replay(&mut self) {
		let queued: HashSet<_> = self.replay_queue.iter().cloned().collect();
		let mut replay: Vec<_> = self
			.replay_desired
			.iter()
			.filter(|(identity, _)| !queued.contains(*identity))
			.map(|(identity, watch)| (watch.watch_path.clone(), identity.clone()))
			.collect();
		replay.sort_by(|(left, _), (right, _)| left.cmp(right));
		self.replay_queue
			.extend(replay.into_iter().map(|(_, identity)| identity));
	}

	fn schedule_retries(&mut self) {
		self.resource_latched = false;
		self.requeue_replay();

		let mut roots: Vec<_> = self
			.retry_roots
			.iter()
			.filter(|root| self.roots.contains(*root))
			.cloned()
			.collect();
		roots.sort_by(|left, right| left.path.cmp(&right.path));
		self.work.extend(roots.into_iter().map(Work::Root));

		let mut candidates: Vec<_> = self
			.retry_candidates
			.iter()
			.filter(|(root, _)| self.roots.contains(root))
			.cloned()
			.collect();
		candidates.sort_by(|(_, left), (_, right)| left.cmp(right));
		self.work
			.extend(candidates.into_iter().map(|(root, path)| Work::Candidate {
				root,
				path,
				transient_retries: TRANSIENT_RETRIES,
			}));
	}

	fn step_replay(&mut self, identity: Identity, result: &mut StepResult) {
		if self.resource_latched {
			self.replay_queue.clear();
			return;
		}
		let Some(mut replay) = self.replay_desired.remove(&identity) else {
			return;
		};

		loop {
			if self.skipped_additions.contains(&replay.watch_path) {
				if let Some((path, _)) = replay
					.logicals
					.iter()
					.find(|(path, _)| !self.skipped_additions.contains(path))
				{
					replay.watch_path.clone_from(path);
				} else {
					return;
				}
			}

			let attempted = replay.watch_path.clone();
			match self.backend.watch(&attempted, replay.mode) {
				Ok(()) => {
					if !self.verify_poll_registration(&attempted, &identity, result) {
						return;
					}
					self.addition_failures.remove(&attempted);
					let logicals = replay
						.logicals
						.iter()
						.map(|(path, _)| path.clone())
						.collect();
					self.physical.insert(
						identity.clone(),
						PhysicalWatch {
							watch_path: attempted,
							logicals,
							guards: HashSet::new(),
							mode: replay.mode,
						},
					);
					for (path, watch) in replay.logicals {
						self.logical.insert(path, watch);
					}
					return;
				}
				Err(error) => {
					if path_not_found(&error) {
						result.errors.extend(notify_multi_path_errors(
							self.kind,
							WatchedPath::non_recursive(attempted.clone()),
							error,
							false,
						));
						self.addition_failures.remove(&attempted);
						self.skipped_additions.remove(&attempted);
						if let Some(path) = self.retain_valid_replay_aliases(
							&mut replay,
							&identity,
							&attempted,
							result,
						) {
							replay.watch_path = path;
							continue;
						}
					} else if let Some(resource) = classify_resource_error(&error) {
						self.resource_latched = true;
						self.replay_queue.clear();
						if !self.resource_reported {
							self.resource_reported = true;
							let error = resource.into_fs_error(error);
							result.errors.push(RuntimeError::FsWatcher {
								kind: self.kind,
								err: error,
							});
						}
						self.replay_desired.insert(identity, replay);
					} else {
						let failures = self.addition_failures.entry(attempted.clone()).or_default();
						*failures = failures.saturating_add(1);
						result.errors.extend(notify_multi_path_errors(
							self.kind,
							WatchedPath::non_recursive(attempted.clone()),
							error,
							false,
						));
						if *failures >= 2 {
							self.skipped_additions.insert(attempted);
						} else {
							self.replay_desired.insert(identity, replay);
							result.rebuild_backend = true;
						}
					}
					return;
				}
			}
		}
	}

	fn retain_valid_replay_aliases(
		&self,
		replay: &mut ReplayWatch,
		identity: &Identity,
		failed: &Path,
		result: &mut StepResult,
	) -> Option<PathBuf> {
		let mut representative = None;
		replay.logicals.retain(|(path, _)| {
			if path == failed
				|| self.skipped_additions.contains(path)
				|| self
					.removed_prefixes
					.iter()
					.any(|prefix| path.starts_with(prefix))
			{
				return false;
			}

			let valid = match self.scanner.classify(path, self.follow_symlinks) {
				Ok(EntryKind::Directory(canonical)) => identity == &Identity::Canonical(canonical),
				Ok(EntryKind::Other) => identity == &Identity::Lexical(path.clone()),
				Ok(EntryKind::NonFollowedSymlink) => false,
				Err(error) => {
					result.errors.push(self.scan_error(path.clone(), error));
					false
				}
			};
			if valid && representative.is_none() {
				representative = Some(path.clone());
			}
			valid
		});
		representative
	}

	fn step_work(&mut self, work: Work, result: &mut StepResult) {
		match work {
			Work::Guard(root) => self.step_guard(root, result),
			Work::Root(root) => self.step_root(root, result),
			Work::Candidate {
				root,
				path,
				transient_retries,
			} => self.step_candidate(root, path, transient_retries, result),
			Work::Scan {
				root,
				path,
				transient_retries,
			} => self.step_scan(root, path, transient_retries, result),
			Work::Probe(path) => self.step_probe(path, result),
		}
	}

	fn classify_explicit_root(
		&self,
		path: &Path,
	) -> Result<ExplicitRootState, (PathBuf, io::Error)> {
		if self.follow_symlinks {
			return match self.scanner.classify(path, true) {
				Ok(entry) => Ok(ExplicitRootState::Entry(entry)),
				Err(error) if error.kind() == io::ErrorKind::NotFound => {
					Ok(ExplicitRootState::Missing {
						path: path.to_owned(),
						error,
					})
				}
				Err(error) => Err((path.to_owned(), error)),
			};
		}

		let mut prefixes: Vec<_> = path
			.ancestors()
			.filter(|prefix| !prefix.as_os_str().is_empty())
			.map(Path::to_owned)
			.collect();
		prefixes.reverse();
		for prefix in prefixes {
			let is_root = prefix == path;
			match self.scanner.classify(&prefix, false) {
				Ok(EntryKind::Directory(identity)) if is_root => {
					return Ok(ExplicitRootState::Entry(EntryKind::Directory(identity)));
				}
				Ok(EntryKind::Directory(_)) => {}
				Ok(EntryKind::Other) if is_root => {
					return Ok(ExplicitRootState::Entry(EntryKind::Other));
				}
				Ok(EntryKind::Other | EntryKind::NonFollowedSymlink) => {
					return Ok(ExplicitRootState::Unsafe);
				}
				Err(error) if error.kind() == io::ErrorKind::NotFound => {
					return Ok(ExplicitRootState::Missing {
						path: prefix,
						error,
					});
				}
				Err(error) => return Err((prefix, error)),
			}
		}
		Ok(ExplicitRootState::Unsafe)
	}

	fn recheck_guarded_root(&mut self, root: Root, result: &mut StepResult) {
		match self.classify_explicit_root(&root.path) {
			Ok(ExplicitRootState::Entry(_)) => {
				self.removed_prefixes
					.retain(|prefix| !root.path.starts_with(prefix));
				self.skipped_additions.remove(&root.path);
				self.addition_failures.remove(&root.path);
				self.work.push_front(Work::Root(root));
			}
			Ok(ExplicitRootState::Missing { .. } | ExplicitRootState::Unsafe) => {}
			Err((path, error)) => {
				result.errors.push(self.scan_error(path, error));
				self.retry_roots.insert(root);
			}
		}
	}

	fn step_guard(&mut self, root: Root, result: &mut StepResult) {
		if !self.is_managed() {
			return;
		}
		if !self.roots.contains(&root) {
			self.remove_root_guard(&root, result);
			return;
		}

		let tombstoned = self
			.removed_prefixes
			.iter()
			.any(|prefix| root.path.starts_with(prefix));
		if !tombstoned {
			match self.classify_explicit_root(&root.path) {
				Ok(ExplicitRootState::Entry(_)) => {
					self.remove_root_guard(&root, result);
					self.work.push_front(Work::Root(root));
					return;
				}
				Ok(ExplicitRootState::Missing { .. } | ExplicitRootState::Unsafe) => {}
				Err((path, error)) => {
					result.errors.push(self.scan_error(path, error));
					self.retry_roots.insert(root);
					return;
				}
			}
		}
		self.ensure_guard(root, result);
	}

	fn ensure_guard(&mut self, root: Root, result: &mut StepResult) {
		let mut found = None;
		if self.follow_symlinks {
			let mut ancestor = root.path.parent().map(Path::to_owned);
			while let Some(path) = ancestor {
				match self.scanner.classify(&path, true) {
					Ok(EntryKind::Directory(identity)) => {
						// Metadata at processing time is authoritative after an atomic
						// exchange, even if a From/Remove callback tombstoned this guard.
						let replaced = self.removed_prefixes.remove(&path);
						found = Some((path, Identity::Canonical(identity), replaced));
						break;
					}
					Ok(EntryKind::NonFollowedSymlink | EntryKind::Other) => {}
					Err(error) if error.kind() == io::ErrorKind::NotFound => {}
					Err(error) => {
						result.errors.push(self.scan_error(path, error));
						self.retry_roots.insert(root);
						return;
					}
				}
				ancestor = path.parent().map(Path::to_owned);
			}
		} else if let Some(parent) = root.path.parent() {
			let mut prefixes: Vec<_> = parent
				.ancestors()
				.filter(|prefix| !prefix.as_os_str().is_empty())
				.map(Path::to_owned)
				.collect();
			prefixes.reverse();
			for path in prefixes {
				match self.scanner.classify(&path, false) {
					Ok(EntryKind::Directory(identity)) => {
						let replaced = self.removed_prefixes.remove(&path);
						found = Some((path, Identity::Canonical(identity), replaced));
					}
					Ok(EntryKind::NonFollowedSymlink | EntryKind::Other) => break,
					Err(error) if error.kind() == io::ErrorKind::NotFound => break,
					Err(error) => {
						result.errors.push(self.scan_error(path, error));
						self.retry_roots.insert(root);
						return;
					}
				}
			}
		}

		let Some((path, identity, replaced)) = found else {
			self.remove_root_guard(&root, result);
			return;
		};

		let same_guard = self.root_guards.get(&root) == Some(&path)
			&& self
				.guards
				.get(&path)
				.map_or(false, |guard| guard.identity == identity);
		let attached = self
			.physical
			.get(&identity)
			.map_or(false, |physical| physical.guards.contains(&path));
		if same_guard && attached && !replaced {
			self.retry_roots.remove(&root);
			self.recheck_guarded_root(root, result);
			return;
		}

		if replaced
			|| self
				.guards
				.get(&path)
				.map_or(false, |guard| guard.identity != identity)
		{
			let old_identity = self.guards.get(&path).map(|guard| guard.identity.clone());
			if let Some(old_identity) = old_identity {
				if !self.detach_guard_registration(&path, &old_identity, result) {
					self.retry_roots.insert(root);
					return;
				}
			}
		}
		if self.root_guards.get(&root) != Some(&path) {
			self.remove_root_guard(&root, result);
			if result.rebuild_backend {
				self.retry_roots.insert(root);
				return;
			}
		}

		let shared = if let Some(physical) = self.physical.get_mut(&identity) {
			physical.guards.insert(path.clone());
			true
		} else {
			false
		};
		if !shared {
			if self.resource_latched || self.skipped_additions.contains(&path) {
				self.retry_roots.insert(root);
				return;
			}
			match self.backend.watch(&path, RecursiveMode::NonRecursive) {
				Ok(()) => {
					if !self.verify_poll_registration(&path, &identity, result) {
						self.work.push_front(Work::Guard(root));
						return;
					}
					self.addition_failures.remove(&path);
					self.physical.insert(
						identity.clone(),
						PhysicalWatch {
							watch_path: path.clone(),
							logicals: HashSet::new(),
							guards: HashSet::from([path.clone()]),
							mode: RecursiveMode::NonRecursive,
						},
					);
				}
				Err(error) => {
					self.retry_roots.insert(root.clone());
					if path_not_found(&error) {
						result.errors.extend(notify_multi_path_errors(
							self.kind,
							WatchedPath::non_recursive(path.clone()),
							error,
							false,
						));
						self.removed_prefixes.insert(path);
						self.work.push_front(Work::Guard(root));
					} else if let Some(resource) = classify_resource_error(&error) {
						self.resource_latched = true;
						if !self.resource_reported {
							self.resource_reported = true;
							let error = resource.into_fs_error(error);
							result.errors.push(RuntimeError::FsWatcher {
								kind: self.kind,
								err: error,
							});
						}
					} else {
						let failures = self.addition_failures.entry(path.clone()).or_default();
						*failures = failures.saturating_add(1);
						result.errors.extend(notify_multi_path_errors(
							self.kind,
							WatchedPath::non_recursive(path.clone()),
							error,
							false,
						));
						if *failures >= 2 {
							self.skipped_additions.insert(path);
						} else {
							result.rebuild_backend = true;
						}
					}
					return;
				}
			}
		}
		self.guards
			.entry(path.clone())
			.and_modify(|guard| {
				guard.identity = identity.clone();
				guard.owners.insert(root.clone());
			})
			.or_insert_with(|| GuardWatch {
				identity,
				owners: HashSet::from([root.clone()]),
			});
		self.root_guards.insert(root.clone(), path);
		self.retry_roots.remove(&root);

		// Close the classify/install exchange race: the root may have appeared
		// between deciding it needed a guard and installing that guard.
		self.recheck_guarded_root(root, result);
	}

	fn detach_guard_registration(
		&mut self,
		path: &Path,
		identity: &Identity,
		result: &mut StepResult,
	) -> bool {
		let Some(physical) = self.physical.get_mut(identity) else {
			return true;
		};
		physical.guards.remove(path);
		let has_owners = !physical.logicals.is_empty() || !physical.guards.is_empty();
		if has_owners {
			if physical.watch_path == path && !physical.logicals.contains(path) {
				// The removed alias is the backend representative. Replay surviving
				// aliases on a fresh backend rather than unwatching their shared watch.
				result.rebuild_backend = true;
				return false;
			}
			return true;
		}

		let Some(physical) = self.physical.remove(identity) else {
			return true;
		};
		if self.try_unwatch(&physical.watch_path, result) {
			self.schedule_retries();
			true
		} else {
			false
		}
	}

	fn remove_root_guard(&mut self, root: &Root, result: &mut StepResult) {
		let Some(path) = self.root_guards.remove(root) else {
			return;
		};
		let remove = if let Some(guard) = self.guards.get_mut(&path) {
			guard.owners.remove(root);
			guard.owners.is_empty()
		} else {
			false
		};
		if !remove {
			return;
		}
		let Some(guard) = self.guards.remove(&path) else {
			return;
		};
		self.detach_guard_registration(&path, &guard.identity, result);
	}

	fn step_root(&mut self, root: Root, result: &mut StepResult) {
		if !self.roots.contains(&root) || self.skipped_additions.contains(&root.path) {
			return;
		}

		if !self.is_managed() {
			if !self.follow_symlinks {
				match self.classify_explicit_root(&root.path) {
					Ok(ExplicitRootState::Entry(_) | ExplicitRootState::Missing { .. }) => {}
					Ok(ExplicitRootState::Unsafe) => {
						self.retry_roots.remove(&root);
						self.queue_remove_prefix(&root.path);
						return;
					}
					Err((path, error)) => {
						result.errors.push(self.scan_error(path, error));
						self.retry_roots.insert(root);
						return;
					}
				}
			}
			let mode = if root.recursive {
				RecursiveMode::Recursive
			} else {
				RecursiveMode::NonRecursive
			};
			match self.add_logical(
				root.path.clone(),
				Identity::Lexical(root.path.clone()),
				root.clone(),
				mode,
				result,
			) {
				AddResult::Added => {
					self.retry_roots.remove(&root);
				}
				AddResult::Skipped | AddResult::Invalidated | AddResult::Rebuild => {
					self.retry_roots.insert(root);
				}
			}
			return;
		}

		let tombstoned = self
			.removed_prefixes
			.iter()
			.any(|prefix| root.path.starts_with(prefix));
		if tombstoned {
			self.queue_remove_prefix(&root.path);
			self.work.push_front(Work::Guard(root));
			return;
		}

		let (identity, scan) = match self.classify_explicit_root(&root.path) {
			Ok(ExplicitRootState::Entry(EntryKind::Directory(identity))) => {
				(Identity::Canonical(identity), root.recursive)
			}
			Ok(ExplicitRootState::Entry(EntryKind::Other)) => {
				(Identity::Lexical(root.path.clone()), false)
			}
			Ok(
				ExplicitRootState::Entry(EntryKind::NonFollowedSymlink) | ExplicitRootState::Unsafe,
			) => {
				self.retry_roots.remove(&root);
				self.queue_remove_prefix(&root.path);
				self.work.push_front(Work::Guard(root));
				return;
			}
			Ok(ExplicitRootState::Missing { path, error }) => {
				result.errors.push(self.scan_error(path, error));
				self.retry_roots.remove(&root);
				self.removed_prefixes.insert(root.path.clone());
				self.queue_remove_prefix(&root.path);
				self.work.push_front(Work::Guard(root));
				return;
			}
			Err((path, error)) => {
				result.errors.push(self.scan_error(path, error));
				self.retry_roots.insert(root.clone());
				if self.has_source_owner(&root.path, &root) {
					self.carry_forward(&root, &root.path);
				}
				// Classification can fail before the explicit root has any backend
				// coverage. Install (or retain) the nearest safe ancestor guard. Guard
				// installation rechecks the root once, providing a bounded transient
				// retry while leaving later recovery event-driven.
				self.ensure_guard(root, result);
				return;
			}
		};

		match self.add_logical(
			root.path.clone(),
			identity.clone(),
			root.clone(),
			RecursiveMode::NonRecursive,
			result,
		) {
			AddResult::Added => {
				self.retry_roots.remove(&root);
				self.remove_root_guard(&root, result);
				if scan && self.mark_seen(&root, identity) {
					let path = root.path.clone();
					self.work.push_back(Work::Scan {
						root,
						path,
						transient_retries: TRANSIENT_RETRIES,
					});
				}
			}
			AddResult::Skipped | AddResult::Rebuild => {
				self.retry_roots.insert(root);
			}
			AddResult::Invalidated => {
				self.retry_roots.remove(&root);
				self.removed_prefixes.insert(root.path.clone());
				self.queue_remove_prefix(&root.path);
				self.work.push_front(Work::Guard(root));
			}
		}
	}

	fn retry_transient_candidate(&mut self, root: &Root, path: &Path, transient_retries: u8) {
		self.retry_candidates
			.remove(&(root.clone(), path.to_owned()));
		self.carry_forward(root, path);
		if transient_retries > 0 {
			self.work.push_back(Work::Candidate {
				root: root.clone(),
				path: path.to_owned(),
				transient_retries: transient_retries - 1,
			});
		}
	}

	fn step_candidate(
		&mut self,
		root: Root,
		path: PathBuf,
		transient_retries: u8,
		result: &mut StepResult,
	) {
		if !self.roots.contains(&root) || !root.recursive {
			self.retry_candidates.remove(&(root, path));
			return;
		}
		if self.skipped_additions.contains(&path) {
			return;
		}

		if self
			.removed_prefixes
			.iter()
			.any(|prefix| path.starts_with(prefix))
		{
			self.retry_candidates.remove(&(root, path));
			return;
		}
		let classification = match self.scanner.classify(&path, self.follow_symlinks) {
			Ok(classification) => classification,
			Err(error) if error.kind() == io::ErrorKind::NotFound => {
				result.errors.push(self.scan_error(path.clone(), error));
				self.retry_candidates.remove(&(root.clone(), path.clone()));
				self.queue_remove_owner_prefix(&root, &path);
				for explicit in self.roots.iter().filter(|item| item.path == path).cloned() {
					self.work.push_back(Work::Guard(explicit));
				}
				return;
			}
			Err(error) => {
				result.errors.push(self.scan_error(path.clone(), error));
				self.retry_transient_candidate(&root, &path, transient_retries);
				return;
			}
		};
		let EntryKind::Directory(identity) = classification else {
			self.retry_candidates.remove(&(root.clone(), path.clone()));
			self.queue_remove_owner_prefix(&root, &path);
			return;
		};

		match self.candidate_allowed(&root, &path) {
			Ok(true) => {}
			Ok(false) => {
				self.retry_candidates.remove(&(root.clone(), path.clone()));
				self.queue_remove_owner_prefix(&root, &path);
				return;
			}
			Err(error) => {
				result.errors.push(error);
				self.retry_transient_candidate(&root, &path, transient_retries);
				return;
			}
		}

		let identity = Identity::Canonical(identity);
		match self.add_logical(
			path.clone(),
			identity.clone(),
			root.clone(),
			RecursiveMode::NonRecursive,
			result,
		) {
			AddResult::Added => {
				self.retry_candidates.remove(&(root.clone(), path.clone()));
				if self.mark_seen(&root, identity) {
					self.work.push_back(Work::Scan {
						root,
						path,
						transient_retries: TRANSIENT_RETRIES,
					});
				}
			}
			AddResult::Skipped | AddResult::Rebuild => {
				self.retry_candidates.insert((root, path));
			}
			AddResult::Invalidated => {
				self.retry_candidates.remove(&(root.clone(), path.clone()));
				self.queue_remove_owner_prefix(&root, &path);
			}
		}
	}

	fn retry_transient_scan(&mut self, root: &Root, path: &Path, transient_retries: u8) {
		self.carry_forward(root, path);
		if transient_retries > 0 {
			self.work.push_back(Work::Scan {
				root: root.clone(),
				path: path.to_owned(),
				transient_retries: transient_retries - 1,
			});
		}
	}

	fn step_scan(
		&mut self,
		root: Root,
		path: PathBuf,
		transient_retries: u8,
		result: &mut StepResult,
	) {
		if !self.roots.contains(&root)
			|| !self
				.logical
				.get(&path)
				.map_or(false, |watch| watch.owners.contains_key(&root))
		{
			return;
		}

		let mut entry_failed = false;
		let kind = self.kind;
		let scan_result = {
			let scanner = &self.scanner;
			let cwd = &self.cwd;
			let work = &mut self.work;
			let errors = &mut result.errors;
			scanner.scan(&path, &mut |entry| match entry {
				Ok(entry_path) => {
					let entry_path = if entry_path.is_absolute() {
						entry_path.normalize()
					} else {
						cwd.join(entry_path).normalize()
					};
					work.push_back(Work::Candidate {
						root: root.clone(),
						path: entry_path,
						transient_retries: TRANSIENT_RETRIES,
					});
				}
				Err(error) => {
					entry_failed = true;
					errors.push(RuntimeError::FsWatcher {
						kind,
						err: FsWatcherError::PathScan {
							path: path.clone(),
							err: error,
						},
					});
				}
			})
		};

		match scan_result {
			Ok(()) => {
				if entry_failed {
					self.retry_transient_scan(&root, &path, transient_retries);
				}
			}
			Err(error) if error.kind() == io::ErrorKind::NotFound => {
				result.errors.push(self.scan_error(path.clone(), error));
				self.retry_candidates.remove(&(root.clone(), path.clone()));
				self.queue_remove_owner_prefix(&root, &path);
				for explicit in self.roots.iter().filter(|item| item.path == path).cloned() {
					self.work.push_back(Work::Guard(explicit));
				}
			}
			Err(error) => {
				result.errors.push(self.scan_error(path.clone(), error));
				self.retry_candidates.remove(&(root.clone(), path.clone()));
				self.retry_transient_scan(&root, &path, transient_retries);
			}
		}
	}

	fn step_probe(&mut self, path: PathBuf, result: &mut StepResult) {
		match self.scanner.classify(&path, self.follow_symlinks) {
			Ok(EntryKind::Directory(_)) => {
				let roots = self.candidate_roots(&path, true);
				// Canonical text can stay unchanged across an atomic replacement;
				// discard the old registration before installing and scanning it.
				self.queue_remove_prefix(&path);
				for root in roots {
					self.seen.remove(&root);
					self.work.push_back(Work::Candidate {
						root,
						path: path.clone(),
						transient_retries: TRANSIENT_RETRIES,
					});
				}
				self.requeue_configured_roots_below(&path);
			}
			Ok(EntryKind::NonFollowedSymlink | EntryKind::Other) => {
				self.queue_remove_prefix(&path);
				self.requeue_configured_roots_below(&path);
			}
			Err(error) if error.kind() == io::ErrorKind::NotFound => {
				self.queue_remove_prefix(&path);
				self.requeue_configured_roots_below(&path);
			}
			Err(error) => result.errors.push(self.scan_error(path, error)),
		}
	}

	fn candidate_allowed(&self, root: &Root, path: &Path) -> Result<bool, RuntimeError> {
		let Ok(relative) = path.strip_prefix(&root.path) else {
			return Ok(false);
		};
		let mut candidate = root.path.clone();
		for component in relative.components() {
			candidate.push(component);
			if !self.filter.check_dir(&candidate)? {
				return Ok(false);
			}
		}
		Ok(true)
	}

	fn candidate_roots(&self, path: &Path, include_self: bool) -> HashSet<Root> {
		let mut roots = HashSet::new();
		let self_path = if include_self { Some(path) } else { None };
		for owner_path in path.parent().into_iter().chain(self_path) {
			if let Some(watch) = self.logical.get(owner_path) {
				roots.extend(
					watch
						.owners
						.iter()
						.filter(|(root, generation)| {
							**generation == self.epoch
								&& root.recursive && self.roots.contains(*root)
						})
						.map(|(root, _)| root.clone()),
				);
			}
		}
		roots
	}

	fn add_logical(
		&mut self,
		path: PathBuf,
		identity: Identity,
		root: Root,
		mode: RecursiveMode,
		result: &mut StepResult,
	) -> AddResult {
		if let Some(watch) = self.logical.get_mut(&path) {
			let mode_matches = self
				.physical
				.get(&watch.identity)
				.map_or(false, |physical| mode_satisfies(physical.mode, mode));
			if watch.identity != identity || !mode_matches {
				self.rebuild_exclusions.insert(path.clone());
				result.rebuild_backend = true;
				return AddResult::Rebuild;
			}
			watch.owners.insert(root, self.epoch);
			self.addition_failures.remove(&path);
			return AddResult::Added;
		}

		if let Some(physical) = self.physical.get_mut(&identity) {
			if !mode_satisfies(physical.mode, mode) {
				result.rebuild_backend = true;
				return AddResult::Rebuild;
			}
			physical.logicals.insert(path.clone());
			self.addition_failures.remove(&path);
			self.logical.insert(
				path,
				LogicalWatch {
					identity,
					owners: HashMap::from([(root, self.epoch)]),
				},
			);
			return AddResult::Added;
		}

		if self.resource_latched || self.skipped_additions.contains(&path) {
			return AddResult::Skipped;
		}

		match self.backend.watch(&path, mode) {
			Ok(()) => {
				if !self.verify_poll_registration(&path, &identity, result) {
					return if result.rebuild_backend {
						AddResult::Rebuild
					} else {
						AddResult::Invalidated
					};
				}
				self.addition_failures.remove(&path);
				self.physical.insert(
					identity.clone(),
					PhysicalWatch {
						watch_path: path.clone(),
						logicals: HashSet::from([path.clone()]),
						guards: HashSet::new(),
						mode,
					},
				);
				self.logical.insert(
					path,
					LogicalWatch {
						identity,
						owners: HashMap::from([(root, self.epoch)]),
					},
				);
				AddResult::Added
			}
			Err(error) => {
				if path_not_found(&error) {
					result.errors.extend(notify_multi_path_errors(
						self.kind,
						WatchedPath::non_recursive(path),
						error,
						false,
					));
					AddResult::Invalidated
				} else if let Some(resource) = classify_resource_error(&error) {
					self.resource_latched = true;
					if !self.resource_reported {
						self.resource_reported = true;
						let error = resource.into_fs_error(error);
						result.errors.push(RuntimeError::FsWatcher {
							kind: self.kind,
							err: error,
						});
					}
					AddResult::Skipped
				} else {
					let failures = self.addition_failures.entry(path.clone()).or_default();
					*failures = failures.saturating_add(1);
					result.errors.extend(notify_multi_path_errors(
						self.kind,
						WatchedPath::non_recursive(path.clone()),
						error,
						false,
					));
					if *failures >= 2 {
						self.skipped_additions.insert(path);
						AddResult::Skipped
					} else {
						result.rebuild_backend = true;
						AddResult::Rebuild
					}
				}
			}
		}
	}

	fn verify_poll_registration(
		&mut self,
		path: &Path,
		expected: &Identity,
		result: &mut StepResult,
	) -> bool {
		if self.backend_kind != notify::WatcherKind::PollWatcher {
			return true;
		}

		let verification = match self.scanner.classify(path, self.follow_symlinks) {
			Ok(verification) => verification,
			Err(error) => {
				result.errors.push(self.scan_error(path.to_owned(), error));
				return self.undo_invalid_poll_registration(path, result);
			}
		};
		let valid = match verification {
			EntryKind::Directory(identity) => expected == &Identity::Canonical(identity),
			EntryKind::Other => expected == &Identity::Lexical(path.to_owned()),
			EntryKind::NonFollowedSymlink => false,
		};
		if valid {
			return true;
		}

		result.errors.push(self.scan_error(
			path.to_owned(),
			io::Error::new(
				io::ErrorKind::InvalidData,
				"path changed while installing polling watch",
			),
		));
		self.undo_invalid_poll_registration(path, result)
	}

	fn undo_invalid_poll_registration(&mut self, path: &Path, result: &mut StepResult) -> bool {
		self.try_unwatch(path, result);
		false
	}

	fn try_unwatch(&mut self, path: &Path, result: &mut StepResult) -> bool {
		match self.backend.unwatch(path) {
			Ok(())
			| Err(notify::Error {
				kind: notify::ErrorKind::WatchNotFound,
				..
			}) => true,
			Err(error) => {
				result.errors.extend(notify_multi_path_errors(
					self.kind,
					WatchedPath::non_recursive(path),
					error,
					true,
				));
				result.rebuild_backend = true;
				false
			}
		}
	}

	fn mark_seen(&mut self, root: &Root, identity: Identity) -> bool {
		self.seen.entry(root.clone()).or_default().insert(identity)
	}

	fn has_source_owner(&self, path: &Path, root: &Root) -> bool {
		self.logical
			.get(path)
			.map_or(false, |watch| watch.owners.contains_key(root))
	}

	fn carry_forward(&mut self, root: &Root, prefix: &Path) {
		for (path, watch) in &mut self.logical {
			if path.starts_with(prefix) && watch.owners.contains_key(root) {
				watch.owners.insert(root.clone(), self.epoch);
			}
		}
	}

	fn prepare_sweep(&mut self) {
		let mut stale = Vec::new();
		for (path, watch) in &self.logical {
			for (root, generation) in &watch.owners {
				if *generation != self.epoch || !self.roots.contains(root) {
					stale.push(SweepEntry {
						path: path.clone(),
						root: root.clone(),
						epoch: self.epoch,
					});
				}
			}
		}
		stale.sort_by(|left, right| {
			right
				.path
				.components()
				.count()
				.cmp(&left.path.components().count())
				.then_with(|| right.path.cmp(&left.path))
		});
		self.sweep = stale.into();
		self.phase = Phase::Sweeping;
	}

	fn queue_remove_owner_prefix(&mut self, root: &Root, prefix: &Path) {
		self.retry_candidates
			.retain(|(owner, path)| owner != root || !path.starts_with(prefix));
		self.work.retain(|work| {
			!matches!(
				work,
				Work::Candidate {
					root: owner, path, ..
				} | Work::Scan {
					root: owner, path, ..
				}
					if owner == root && path.starts_with(prefix)
			)
		});
		let mut paths: Vec<_> = self
			.logical
			.iter()
			.filter(|(path, watch)| path.starts_with(prefix) && watch.owners.contains_key(root))
			.map(|(path, _)| path.clone())
			.collect();
		paths.sort_by_key(|path| Reverse(path.components().count()));
		self.pending_owner_removals
			.extend(paths.into_iter().map(|path| (path, root.clone())));
	}

	fn queue_remove_prefix(&mut self, prefix: &Path) {
		let configured = &self.roots;
		self.work.retain(|work| match work {
			Work::Root(root) | Work::Guard(root) if configured.contains(root) => true,
			_ => !work.path().starts_with(prefix),
		});
		self.retry_candidates
			.retain(|(_, path)| !path.starts_with(prefix));
		for replay in self.replay_desired.values_mut() {
			replay
				.logicals
				.retain(|(path, _)| !path.starts_with(prefix));
			if !replay
				.logicals
				.iter()
				.any(|(path, _)| path == &replay.watch_path)
			{
				if let Some((path, _)) = replay.logicals.first() {
					replay.watch_path.clone_from(path);
				}
			}
		}
		self.replay_desired
			.retain(|_, replay| !replay.logicals.is_empty());
		self.replay_queue
			.retain(|identity| self.replay_desired.contains_key(identity));

		let mut paths: Vec<_> = self
			.logical
			.keys()
			.filter(|path| path.starts_with(prefix))
			.cloned()
			.collect();
		paths.sort_by(|left, right| {
			right
				.components()
				.count()
				.cmp(&left.components().count())
				.then_with(|| right.cmp(left))
		});
		for path in paths {
			if self.pending_removal_set.insert(path.clone()) {
				self.pending_removals.push_back(path);
			}
		}
	}

	fn remove_owner_force(&mut self, entry: SweepEntry, result: &mut StepResult) {
		let remove_logical = if let Some(watch) = self.logical.get_mut(&entry.path) {
			watch.owners.remove(&entry.root);
			watch.owners.is_empty()
		} else {
			false
		};
		if remove_logical {
			self.remove_logical(&entry.path, result);
		}
	}

	fn remove_owner(&mut self, entry: SweepEntry, result: &mut StepResult) {
		if entry.epoch != self.epoch {
			return;
		}
		let remove_logical = if let Some(watch) = self.logical.get_mut(&entry.path) {
			if watch.owners.get(&entry.root).copied() != Some(entry.epoch) {
				watch.owners.remove(&entry.root);
			}
			watch.owners.is_empty()
		} else {
			false
		};
		if remove_logical {
			self.remove_logical(&entry.path, result);
		}
	}

	fn remove_logical(&mut self, path: &Path, result: &mut StepResult) {
		let Some(logical) = self.logical.remove(path) else {
			return;
		};
		let (remove_physical, removed_representative) =
			if let Some(physical) = self.physical.get_mut(&logical.identity) {
				physical.logicals.remove(path);
				let has_owners = !physical.logicals.is_empty() || !physical.guards.is_empty();
				(
					!has_owners,
					physical.watch_path == path && has_owners && !physical.guards.contains(path),
				)
			} else {
				(false, false)
			};
		if removed_representative {
			// The surviving aliases cannot rely on a registration whose lexical
			// representative has disappeared. Replay them on a fresh backend.
			result.rebuild_backend = true;
			return;
		}
		if !remove_physical {
			return;
		}

		let Some(physical) = self.physical.remove(&logical.identity) else {
			return;
		};
		if self.try_unwatch(&physical.watch_path, result) {
			self.schedule_retries();
		}
	}

	fn settle(&mut self) {
		self.phase = Phase::Settled;
	}

	const fn scan_error(&self, path: PathBuf, error: io::Error) -> RuntimeError {
		RuntimeError::FsWatcher {
			kind: self.kind,
			err: FsWatcherError::PathScan { path, err: error },
		}
	}

	fn absolute(&self, path: &Path) -> PathBuf {
		if path.is_absolute() {
			path.normalize()
		} else {
			self.cwd.join(path).normalize()
		}
	}
}

#[cfg(test)]
mod tests {
	use std::sync::{Arc, Mutex};

	use notify::ErrorKind;
	use watchexec_events::{Event, Priority};

	use super::*;

	#[derive(Clone, Debug, PartialEq, Eq)]
	enum Operation {
		Watch(PathBuf, RecursiveMode),
		Unwatch(PathBuf),
		Classify(PathBuf),
		Scan(PathBuf),
	}

	#[derive(Default)]
	struct FakeBackendState {
		operations: Vec<Operation>,
		resource_failure: Option<PathBuf>,
		path_not_found_failures: HashMap<PathBuf, usize>,
		generic_failures: HashMap<PathBuf, usize>,
		generic_unwatch_failures: HashMap<PathBuf, usize>,
		watch_not_found: HashSet<PathBuf>,
	}

	struct FakeBackend(Arc<Mutex<FakeBackendState>>);

	impl Backend for FakeBackend {
		fn watch(&mut self, path: &Path, mode: RecursiveMode) -> notify::Result<()> {
			let mut state = self.0.lock().unwrap();
			state
				.operations
				.push(Operation::Watch(path.to_owned(), mode));
			if state.resource_failure.as_deref() == Some(path) {
				Err(notify::Error::new(ErrorKind::MaxFilesWatch))
			} else if let Some(remaining) = state.path_not_found_failures.get_mut(path) {
				if *remaining > 0 {
					*remaining -= 1;
					return Err(notify::Error::new(ErrorKind::PathNotFound));
				}
				Ok(())
			} else if let Some(remaining) = state.generic_failures.get_mut(path) {
				if *remaining > 0 {
					*remaining -= 1;
					return Err(notify::Error::generic("fake watch failure"));
				}
				Ok(())
			} else {
				Ok(())
			}
		}

		fn unwatch(&mut self, path: &Path) -> notify::Result<()> {
			let mut state = self.0.lock().unwrap();
			state.operations.push(Operation::Unwatch(path.to_owned()));
			if let Some(remaining) = state.generic_unwatch_failures.get_mut(path) {
				if *remaining > 0 {
					*remaining -= 1;
					return Err(notify::Error::generic("fake unwatch failure"));
				}
			}
			if state.watch_not_found.contains(path) {
				Err(notify::Error::new(ErrorKind::WatchNotFound))
			} else {
				Ok(())
			}
		}
	}

	#[derive(Default)]
	struct FakeScannerState {
		directories: HashMap<PathBuf, PathBuf>,
		entries: HashMap<PathBuf, Vec<PathBuf>>,
		not_found: HashSet<PathBuf>,
		not_found_after: HashMap<PathBuf, usize>,
		non_followed_symlinks: HashSet<PathBuf>,
		classify_errors: HashSet<PathBuf>,
		classify_errors_after: HashMap<PathBuf, usize>,
		scan_errors: HashSet<PathBuf>,
		operations: Vec<Operation>,
	}

	struct FakeScanner(Arc<Mutex<FakeScannerState>>);

	impl Scanner for FakeScanner {
		fn classify(&self, path: &Path, follow_symlinks: bool) -> io::Result<EntryKind> {
			let mut state = self.0.lock().unwrap();
			state.operations.push(Operation::Classify(path.to_owned()));
			let delayed_missing = state
				.not_found_after
				.get_mut(path)
				.map_or(false, |remaining| {
					if *remaining == 0 {
						true
					} else {
						*remaining -= 1;
						false
					}
				});
			let delayed_error =
				state
					.classify_errors_after
					.get_mut(path)
					.map_or(false, |remaining| {
						if *remaining == 0 {
							true
						} else {
							*remaining -= 1;
							false
						}
					});
			if state.not_found.contains(path) || delayed_missing {
				Err(io::Error::new(io::ErrorKind::NotFound, "fake missing path"))
			} else if state.non_followed_symlinks.contains(path) && !follow_symlinks {
				Ok(EntryKind::NonFollowedSymlink)
			} else if state.classify_errors.contains(path) || delayed_error {
				Err(io::Error::new(
					io::ErrorKind::PermissionDenied,
					"fake classify failure",
				))
			} else {
				Ok(state
					.directories
					.get(path)
					.cloned()
					.map_or(EntryKind::Other, EntryKind::Directory))
			}
		}

		fn scan(&self, path: &Path, visit: &mut dyn FnMut(io::Result<PathBuf>)) -> io::Result<()> {
			let entries = {
				let mut state = self.0.lock().unwrap();
				state.operations.push(Operation::Scan(path.to_owned()));
				if state.scan_errors.contains(path) {
					return Err(io::Error::new(
						io::ErrorKind::PermissionDenied,
						"fake scan failure",
					));
				}
				state.entries.get(path).cloned().unwrap_or_default()
			};
			for entry in entries {
				visit(Ok(entry));
			}
			Ok(())
		}
	}

	#[derive(Debug)]
	struct TestFilter {
		denied: HashSet<PathBuf>,
	}

	impl Filterer for TestFilter {
		fn check_dir(&self, path: &Path) -> Result<bool, RuntimeError> {
			Ok(!self.denied.contains(path))
		}

		fn check_event(&self, _event: &Event, _priority: Priority) -> Result<bool, RuntimeError> {
			Ok(true)
		}
	}

	fn filter(denied: impl IntoIterator<Item = &'static str>) -> Arc<dyn Filterer> {
		Arc::new(TestFilter {
			denied: denied.into_iter().map(PathBuf::from).collect(),
		})
	}

	fn directory(state: &Arc<Mutex<FakeScannerState>>, path: &str) {
		state
			.lock()
			.unwrap()
			.directories
			.insert(path.into(), path.into());
	}

	fn entries(state: &Arc<Mutex<FakeScannerState>>, path: &str, entries: &[&str]) {
		state
			.lock()
			.unwrap()
			.entries
			.insert(path.into(), entries.iter().map(PathBuf::from).collect());
	}

	fn fixture() -> (
		Recursor,
		Arc<Mutex<FakeBackendState>>,
		Arc<Mutex<FakeScannerState>>,
	) {
		let backend = Arc::new(Mutex::new(FakeBackendState::default()));
		let scanner = Arc::new(Mutex::new(FakeScannerState::default()));
		directory(&scanner, "/");
		let recursor = Recursor::new(
			Box::new(FakeBackend(backend.clone())),
			Box::new(FakeScanner(scanner.clone())),
			Watcher::Native,
			notify::WatcherKind::Inotify,
			true,
			PathBuf::from("/work"),
			filter([]),
		);
		(recursor, backend, scanner)
	}

	fn drain(recursor: &mut Recursor) -> Vec<RuntimeError> {
		let mut errors = Vec::new();
		for _ in 0..256 {
			if !recursor.has_work() {
				return errors;
			}
			let step = recursor.step();
			assert!(!step.rebuild_backend, "unexpected backend rebuild");
			errors.extend(step.errors);
		}
		panic!("recursor did not settle");
	}

	fn run_until_rebuild(recursor: &mut Recursor) -> Vec<RuntimeError> {
		let mut errors = Vec::new();
		for _ in 0..256 {
			assert!(recursor.has_work(), "recursor settled before rebuilding");
			let step = recursor.step();
			errors.extend(step.errors);
			if step.rebuild_backend {
				return errors;
			}
		}
		panic!("recursor did not request a rebuild");
	}

	fn watched(backend: &Arc<Mutex<FakeBackendState>>, path: &str) -> usize {
		backend
			.lock()
			.unwrap()
			.operations
			.iter()
			.filter(
				|operation| matches!(operation, Operation::Watch(watched, _) if watched == Path::new(path)),
			)
			.count()
	}

	fn unwatched(backend: &Arc<Mutex<FakeBackendState>>, path: &str) -> usize {
		backend
			.lock()
			.unwrap()
			.operations
			.iter()
			.filter(
				|operation| matches!(operation, Operation::Unwatch(watched) if watched == Path::new(path)),
			)
			.count()
	}

	#[test]
	fn watches_each_directory_before_scanning_it() {
		let (mut recursor, backend, scanner) = fixture();
		directory(&scanner, "/root");
		directory(&scanner, "/root/child");
		entries(&scanner, "/root", &["/root/child"]);

		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));

		// The first bounded step watches the root. Classification is allowed
		// before registration, but directory contents are not read until the
		// following step.
		recursor.step();
		assert_eq!(watched(&backend, "/root"), 1);
		assert!(!scanner
			.lock()
			.unwrap()
			.operations
			.contains(&Operation::Scan("/root".into())));

		recursor.step();
		assert!(scanner
			.lock()
			.unwrap()
			.operations
			.contains(&Operation::Scan("/root".into())));

		// The discovered child follows the same watch-then-scan ordering.
		recursor.step();
		assert_eq!(watched(&backend, "/root/child"), 1);
		assert!(!scanner
			.lock()
			.unwrap()
			.operations
			.contains(&Operation::Scan("/root/child".into())));
		recursor.step();
		assert!(scanner
			.lock()
			.unwrap()
			.operations
			.contains(&Operation::Scan("/root/child".into())));
		drain(&mut recursor);

		assert!(backend.lock().unwrap().operations.iter().all(|operation| {
			!matches!(operation, Operation::Watch(_, RecursiveMode::Recursive))
		}));
	}

	#[test]
	fn ignored_descendant_is_pruned_but_explicit_root_is_not() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/ignored", "/root/ignored/nested"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/ignored"]);
		entries(&scanner, "/root/ignored", &["/root/ignored/nested"]);

		recursor.reconcile(
			&[WatchedPath::recursive("/root")],
			filter(["/root/ignored"]),
		);
		drain(&mut recursor);
		assert_eq!(watched(&backend, "/root/ignored"), 0);

		// Adding the ignored directory as a configured root bypasses check_dir,
		// while its own accepted descendants are still traversed.
		recursor.reconcile(
			&[
				WatchedPath::recursive("/root"),
				WatchedPath::recursive("/root/ignored"),
			],
			filter(["/root/ignored"]),
		);
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/root/ignored"), 1);
		assert_eq!(watched(&backend, "/root/ignored/nested"), 1);
	}

	#[test]
	fn a_local_scan_failure_does_not_stop_good_siblings() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/good"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/bad", "/root/good"]);
		scanner
			.lock()
			.unwrap()
			.classify_errors
			.insert("/root/bad".into());

		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		let errors = drain(&mut recursor);

		assert!(errors.iter().any(|error| matches!(
			error,
			RuntimeError::FsWatcher {
				err: FsWatcherError::PathScan { path, .. },
				..
			} if path == Path::new("/root/bad")
		)));
		assert_eq!(watched(&backend, "/root/good"), 1);
	}

	#[test]
	fn transient_child_classification_failure_is_retried_once() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/child", "/root/child/grand"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/child"]);
		entries(&scanner, "/root/child", &["/root/child/grand"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		recursor.step();
		recursor.step();
		scanner
			.lock()
			.unwrap()
			.classify_errors
			.insert("/root/child".into());

		let failed = recursor.step();
		assert_eq!(failed.errors.len(), 1);
		scanner
			.lock()
			.unwrap()
			.classify_errors
			.remove(Path::new("/root/child"));
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/root/child"), 1);
		assert_eq!(watched(&backend, "/root/child/grand"), 1);
	}

	#[test]
	fn transient_child_scan_failure_is_retried_once() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/child", "/root/child/grand"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/child"]);
		entries(&scanner, "/root/child", &["/root/child/grand"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		recursor.step();
		recursor.step();
		recursor.step();
		scanner
			.lock()
			.unwrap()
			.scan_errors
			.insert("/root/child".into());

		let failed = recursor.step();
		assert_eq!(failed.errors.len(), 1);
		scanner
			.lock()
			.unwrap()
			.scan_errors
			.remove(Path::new("/root/child"));
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/root/child/grand"), 1);
	}

	#[test]
	fn persistent_child_errors_are_bounded_and_keep_parent_coverage() {
		let (mut recursor, backend, scanner) = fixture();
		directory(&scanner, "/root");
		directory(&scanner, "/root/child");
		entries(&scanner, "/root", &["/root/child"]);
		scanner
			.lock()
			.unwrap()
			.classify_errors
			.insert("/root/child".into());

		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		let errors = drain(&mut recursor);
		let classifications = scanner
			.lock()
			.unwrap()
			.operations
			.iter()
			.filter(|operation| operation == &&Operation::Classify("/root/child".into()))
			.count();

		assert_eq!(errors.len(), 2);
		assert_eq!(classifications, 2);
		assert!(recursor.logical.contains_key(Path::new("/root")));
		assert_eq!(unwatched(&backend, "/root"), 0);
	}

	#[test]
	fn second_epoch_classification_failure_preserves_known_subtree() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/child", "/root/child/grand"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/child"]);
		entries(&scanner, "/root/child", &["/root/child/grand"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		scanner
			.lock()
			.unwrap()
			.classify_errors
			.insert("/root/child".into());
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		let errors = drain(&mut recursor);

		assert_eq!(errors.len(), 2);
		assert_eq!(unwatched(&backend, "/root/child"), 0);
		assert_eq!(unwatched(&backend, "/root/child/grand"), 0);
		assert!(recursor
			.logical
			.contains_key(Path::new("/root/child/grand")));
	}

	#[test]
	fn second_epoch_scan_failure_preserves_known_descendants() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/child", "/root/child/grand"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/child"]);
		entries(&scanner, "/root/child", &["/root/child/grand"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		scanner
			.lock()
			.unwrap()
			.scan_errors
			.insert("/root/child".into());
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		let errors = drain(&mut recursor);

		assert_eq!(errors.len(), 2);
		assert_eq!(unwatched(&backend, "/root/child/grand"), 0);
		assert!(recursor
			.logical
			.contains_key(Path::new("/root/child/grand")));
	}

	#[test]
	fn nonrecursive_root_is_watched_once_without_scanning() {
		let (mut recursor, backend, scanner) = fixture();
		directory(&scanner, "/root");
		entries(&scanner, "/root", &["/root/child"]);

		recursor.reconcile(&[WatchedPath::non_recursive("/root")], filter([]));
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/root"), 1);
		assert!(!scanner
			.lock()
			.unwrap()
			.operations
			.iter()
			.any(|operation| matches!(operation, Operation::Scan(_))));
	}

	#[test]
	fn native_poll_fallback_verifies_success_against_metadata() {
		let (mut recursor, backend, scanner) = fixture();
		recursor.backend_kind = notify::WatcherKind::PollWatcher;
		directory(&scanner, "/root");
		scanner
			.lock()
			.unwrap()
			.not_found_after
			.insert("/root".into(), 1);

		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		let errors = drain(&mut recursor);

		assert_eq!(watched(&backend, "/root"), 1);
		assert_eq!(unwatched(&backend, "/root"), 1);
		assert!(!recursor.logical.contains_key(Path::new("/root")));
		assert!(errors.iter().any(|error| matches!(
			error,
			RuntimeError::FsWatcher {
				err: FsWatcherError::PathScan { path, .. },
				..
			} if path == Path::new("/root")
		)));
	}

	#[test]
	fn poll_permission_error_does_not_commit_unverified_registration() {
		let (mut recursor, backend, scanner) = fixture();
		recursor.backend_kind = notify::WatcherKind::PollWatcher;
		directory(&scanner, "/root");
		scanner
			.lock()
			.unwrap()
			.classify_errors_after
			.insert("/root".into(), 1);

		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		let errors = drain(&mut recursor);

		assert_eq!(watched(&backend, "/root"), 1);
		assert_eq!(unwatched(&backend, "/root"), 1);
		assert!(!recursor.logical.contains_key(Path::new("/root")));
		assert_eq!(
			recursor.root_guards.values().next(),
			Some(&PathBuf::from("/"))
		);
		assert!(errors.iter().any(|error| matches!(
			error,
			RuntimeError::FsWatcher {
				err: FsWatcherError::PathScan { path, err },
				..
			} if path == Path::new("/root") && err.kind() == io::ErrorKind::PermissionDenied
		)));
	}

	#[test]
	fn unmanaged_backend_installs_complete_roots_with_requested_modes() {
		let backend = Arc::new(Mutex::new(FakeBackendState::default()));
		let scanner = Arc::new(Mutex::new(FakeScannerState::default()));
		let mut recursor = Recursor::new(
			Box::new(FakeBackend(backend.clone())),
			Box::new(FakeScanner(scanner.clone())),
			Watcher::Native,
			notify::WatcherKind::Fsevent,
			true,
			PathBuf::from("/work"),
			filter([]),
		);
		recursor.reconcile(
			&[
				WatchedPath::recursive("/one"),
				WatchedPath::non_recursive("/two"),
			],
			filter([]),
		);
		drain(&mut recursor);

		let operations = &backend.lock().unwrap().operations;
		assert!(operations.contains(&Operation::Watch("/one".into(), RecursiveMode::Recursive)));
		assert!(operations.contains(&Operation::Watch(
			"/two".into(),
			RecursiveMode::NonRecursive
		)));
		assert!(scanner.lock().unwrap().operations.is_empty());
	}

	#[test]
	fn unmanaged_same_value_epoch_retries_resource_failed_root() {
		let backend = Arc::new(Mutex::new(FakeBackendState::default()));
		let scanner = Arc::new(Mutex::new(FakeScannerState::default()));
		backend.lock().unwrap().resource_failure = Some("/root".into());
		let mut recursor = Recursor::new(
			Box::new(FakeBackend(backend.clone())),
			Box::new(FakeScanner(scanner)),
			Watcher::Native,
			notify::WatcherKind::Fsevent,
			true,
			PathBuf::from("/work"),
			filter([]),
		);
		let pathset = [WatchedPath::recursive("/root")];
		recursor.reconcile(&pathset, filter([]));
		let first = drain(&mut recursor);
		assert_eq!(first.len(), 1);
		assert!(recursor.needs_retry());

		backend.lock().unwrap().resource_failure = None;
		recursor.reconcile(&pathset, filter([]));
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/root"), 2);
		assert!(recursor.logical.contains_key(Path::new("/root")));
		assert!(!recursor.needs_retry());
	}

	#[test]
	fn recursive_and_nonrecursive_same_root_share_one_watch() {
		let (mut recursor, backend, scanner) = fixture();
		directory(&scanner, "/root");
		recursor.reconcile(
			&[
				WatchedPath::non_recursive("/root"),
				WatchedPath::recursive("/root"),
			],
			filter([]),
		);
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/root"), 1);
		assert!(scanner
			.lock()
			.unwrap()
			.operations
			.contains(&Operation::Scan("/root".into())));
	}

	#[test]
	fn queued_sweep_does_not_remove_refreshed_owner() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/child"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/child"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		let root = recursor.roots.iter().next().unwrap().clone();
		recursor.work.clear();
		recursor.prepare_sweep();
		assert!(!recursor.sweep.is_empty());
		recursor
			.logical
			.get_mut(Path::new("/root/child"))
			.unwrap()
			.owners
			.insert(root, recursor.epoch);
		drain(&mut recursor);

		assert_eq!(unwatched(&backend, "/root/child"), 0);
		assert!(recursor.logical.contains_key(Path::new("/root/child")));
	}

	#[test]
	fn overlapping_roots_share_registrations_and_ownership() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/child", "/root/child/grand"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/child"]);
		entries(&scanner, "/root/child", &["/root/child/grand"]);

		recursor.reconcile(
			&[
				WatchedPath::recursive("/root"),
				WatchedPath::recursive("/root/child"),
			],
			filter([]),
		);
		drain(&mut recursor);
		assert_eq!(watched(&backend, "/root/child"), 1);

		recursor.reconcile(&[WatchedPath::recursive("/root/child")], filter([]));
		drain(&mut recursor);
		assert_eq!(unwatched(&backend, "/root/child"), 0);
		assert_eq!(unwatched(&backend, "/root/child/grand"), 0);
		assert_eq!(unwatched(&backend, "/root"), 1);
	}

	#[test]
	fn recursive_file_discovery_keeps_explicit_file_owner() {
		let (mut recursor, backend, scanner) = fixture();
		directory(&scanner, "/root");
		entries(&scanner, "/root", &["/root/file"]);
		recursor.reconcile(
			&[
				WatchedPath::recursive("/root"),
				WatchedPath::non_recursive("/root/file"),
			],
			filter([]),
		);
		drain(&mut recursor);

		let explicit = Root {
			path: "/root/file".into(),
			recursive: false,
		};
		let recursive = Root {
			path: "/root".into(),
			recursive: true,
		};
		let file = recursor.logical.get(Path::new("/root/file")).unwrap();
		assert!(file.owners.contains_key(&explicit));
		assert!(!file.owners.contains_key(&recursive));
		assert_eq!(watched(&backend, "/root/file"), 1);
		assert_eq!(unwatched(&backend, "/root/file"), 0);
	}

	#[test]
	fn removal_uses_known_prefix_without_metadata() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/gone", "/root/gone/child"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/gone"]);
		entries(&scanner, "/root/gone", &["/root/gone/child"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);
		let classifications = scanner.lock().unwrap().operations.len();

		recursor.topology_remove("/root/gone".into());
		drain(&mut recursor);

		assert_eq!(scanner.lock().unwrap().operations.len(), classifications);
		assert_eq!(unwatched(&backend, "/root/gone/child"), 1);
		assert_eq!(unwatched(&backend, "/root/gone"), 1);
	}

	#[test]
	fn prefix_removal_finishes_after_generic_unwatch_rebuild() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/gone", "/root/gone/child"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/gone"]);
		entries(&scanner, "/root/gone", &["/root/gone/child"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);
		backend
			.lock()
			.unwrap()
			.generic_unwatch_failures
			.insert("/root/gone/child".into(), 1);

		recursor.topology_remove("/root/gone".into());
		assert_eq!(run_until_rebuild(&mut recursor).len(), 1);
		let replacement = Arc::new(Mutex::new(FakeBackendState::default()));
		recursor.replace_backend(Box::new(FakeBackend(replacement.clone())));
		drain(&mut recursor);

		assert_eq!(watched(&replacement, "/root/gone"), 0);
		assert_eq!(watched(&replacement, "/root/gone/child"), 0);
		assert!(!recursor.logical.contains_key(Path::new("/root/gone")));
		assert!(!recursor.logical.contains_key(Path::new("/root/gone/child")));
	}

	#[test]
	fn already_removed_backend_watch_converges_without_error() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/gone"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/gone"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);
		backend
			.lock()
			.unwrap()
			.watch_not_found
			.insert("/root/gone".into());

		recursor.topology_remove("/root/gone".into());
		let errors = drain(&mut recursor);

		assert!(errors.is_empty());
		assert!(!recursor.logical.contains_key(Path::new("/root/gone")));
		assert_eq!(unwatched(&backend, "/root/gone"), 1);
	}

	#[test]
	fn configured_directory_root_is_reacquired_after_delete_recreate() {
		let (mut recursor, backend, scanner) = fixture();
		directory(&scanner, "/root");
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);
		assert_eq!(watched(&backend, "/"), 0);

		scanner.lock().unwrap().not_found.insert("/root".into());
		recursor.topology_remove("/root".into());
		drain(&mut recursor);
		assert!(!recursor.logical.contains_key(Path::new("/root")));
		assert_eq!(
			recursor.root_guards.values().next(),
			Some(&PathBuf::from("/"))
		);
		assert_eq!(watched(&backend, "/"), 1);

		scanner.lock().unwrap().not_found.remove(Path::new("/root"));
		recursor.topology_create("/root".into());
		drain(&mut recursor);
		assert_eq!(watched(&backend, "/root"), 2);
		assert!(recursor.logical.contains_key(Path::new("/root")));
		assert!(recursor.root_guards.is_empty());
	}

	#[test]
	fn from_only_atomic_root_exchange_reacquires_without_create() {
		let (mut recursor, backend, scanner) = fixture();
		scanner
			.lock()
			.unwrap()
			.directories
			.insert("/root".into(), "/identity/a".into());
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		scanner
			.lock()
			.unwrap()
			.directories
			.insert("/root".into(), "/identity/b".into());
		recursor.topology_remove("/root".into());
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/root"), 2);
		assert_eq!(
			recursor.logical.get(Path::new("/root")).unwrap().identity,
			Identity::Canonical("/identity/b".into())
		);
		assert!(recursor.root_guards.is_empty());
	}

	#[test]
	fn missing_root_guard_moves_to_nearest_existing_ancestor() {
		let (mut recursor, backend, scanner) = fixture();
		scanner
			.lock()
			.unwrap()
			.not_found
			.extend([PathBuf::from("/a"), PathBuf::from("/a/b")]);
		recursor.reconcile(&[WatchedPath::recursive("/a/b")], filter([]));
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/a/b"), 0);
		assert_eq!(watched(&backend, "/"), 1);
		assert_eq!(
			recursor.root_guards.values().next(),
			Some(&PathBuf::from("/"))
		);

		directory(&scanner, "/a");
		scanner.lock().unwrap().not_found.remove(Path::new("/a"));
		recursor.topology_create("/a".into());
		drain(&mut recursor);
		assert_eq!(unwatched(&backend, "/"), 1);
		assert_eq!(watched(&backend, "/a"), 1);
		assert_eq!(
			recursor.root_guards.values().next(),
			Some(&PathBuf::from("/a"))
		);

		scanner.lock().unwrap().not_found.insert("/a".into());
		recursor.topology_remove("/a".into());
		drain(&mut recursor);
		assert_eq!(unwatched(&backend, "/a"), 1);
		assert_eq!(watched(&backend, "/"), 2);

		scanner.lock().unwrap().not_found.remove(Path::new("/a"));
		scanner.lock().unwrap().not_found.remove(Path::new("/a/b"));
		directory(&scanner, "/a/b");
		recursor.topology_create("/a".into());
		recursor.topology_create("/a/b".into());
		drain(&mut recursor);
		assert_eq!(watched(&backend, "/a/b"), 1);
		assert!(recursor.root_guards.is_empty());
	}

	#[test]
	fn missing_outer_root_does_not_discard_queued_nested_root() {
		let (mut recursor, backend, scanner) = fixture();
		scanner
			.lock()
			.unwrap()
			.not_found
			.extend([PathBuf::from("/a"), PathBuf::from("/a/b")]);

		recursor.reconcile(
			&[
				WatchedPath::non_recursive("/a"),
				WatchedPath::recursive("/a/b"),
			],
			filter([]),
		);
		drain(&mut recursor);

		assert_eq!(recursor.root_guards.len(), 2);
		assert!(recursor
			.roots
			.iter()
			.all(|root| recursor.root_guards.get(root) == Some(&PathBuf::from("/"))));
		assert_eq!(watched(&backend, "/"), 1);
		assert_eq!(watched(&backend, "/a"), 0);
		assert_eq!(watched(&backend, "/a/b"), 0);
	}

	#[test]
	fn guarded_nested_root_advances_on_one_sided_move_to_ancestor() {
		let (mut recursor, backend, scanner) = fixture();
		scanner
			.lock()
			.unwrap()
			.not_found
			.extend([PathBuf::from("/a"), PathBuf::from("/a/b")]);
		recursor.reconcile(&[WatchedPath::recursive("/a/b")], filter([]));
		drain(&mut recursor);
		assert_eq!(
			recursor
				.root_guards
				.get(recursor.roots.iter().next().unwrap()),
			Some(&PathBuf::from("/"))
		);

		directory(&scanner, "/a");
		scanner.lock().unwrap().not_found.remove(Path::new("/a"));
		// A one-sided rename To is conservatively delivered as ambiguous.
		recursor.topology_ambiguous("/a".into());
		drain(&mut recursor);
		assert_eq!(
			recursor
				.root_guards
				.get(recursor.roots.iter().next().unwrap()),
			Some(&PathBuf::from("/a"))
		);
		assert_eq!(watched(&backend, "/a"), 1);

		directory(&scanner, "/a/b");
		scanner.lock().unwrap().not_found.remove(Path::new("/a/b"));
		recursor.topology_create("/a/b".into());
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/a/b"), 1);
		assert!(recursor.logical.contains_key(Path::new("/a/b")));
		assert!(recursor.root_guards.is_empty());
	}

	#[test]
	fn atomic_exchange_of_guarded_ancestor_reregisters_closer_guard() {
		let (mut recursor, backend, scanner) = fixture();
		scanner
			.lock()
			.unwrap()
			.directories
			.extend([(PathBuf::from("/a"), PathBuf::from("/identity/old"))]);
		scanner.lock().unwrap().not_found.insert("/a/b".into());
		recursor.reconcile(&[WatchedPath::recursive("/a/b")], filter([]));
		drain(&mut recursor);
		assert_eq!(watched(&backend, "/a"), 1);

		scanner
			.lock()
			.unwrap()
			.directories
			.insert("/a".into(), "/identity/new".into());
		// A From may be all that is delivered for an atomic exchange. Current
		// metadata must supersede the callback tombstone for the guard path.
		recursor.topology_remove("/a".into());
		drain(&mut recursor);

		assert_eq!(unwatched(&backend, "/a"), 1);
		assert_eq!(watched(&backend, "/a"), 2);
		assert_eq!(
			recursor.root_guards.values().next(),
			Some(&PathBuf::from("/a"))
		);

		directory(&scanner, "/a/b");
		scanner.lock().unwrap().not_found.remove(Path::new("/a/b"));
		recursor.topology_create("/a/b".into());
		drain(&mut recursor);
		assert!(recursor.logical.contains_key(Path::new("/a/b")));
		assert!(recursor.root_guards.is_empty());
	}

	#[test]
	fn guard_permission_error_preserves_closer_existing_coverage() {
		let (mut recursor, backend, scanner) = fixture();
		directory(&scanner, "/a");
		scanner.lock().unwrap().not_found.insert("/a/b".into());
		recursor.reconcile(&[WatchedPath::recursive("/a/b")], filter([]));
		drain(&mut recursor);
		assert_eq!(watched(&backend, "/a"), 1);

		scanner.lock().unwrap().classify_errors.insert("/a".into());
		let root = recursor.roots.iter().next().unwrap().clone();
		recursor.work.push_back(Work::Guard(root));
		let errors = drain(&mut recursor);

		assert_eq!(errors.len(), 1);
		assert_eq!(unwatched(&backend, "/a"), 0);
		assert_eq!(watched(&backend, "/"), 0);
		assert_eq!(
			recursor.root_guards.values().next(),
			Some(&PathBuf::from("/a"))
		);
	}

	#[test]
	fn initial_root_permission_error_installs_guard_and_retries_once() {
		let (mut recursor, backend, scanner) = fixture();
		scanner
			.lock()
			.unwrap()
			.classify_errors
			.insert("/denied".into());

		recursor.reconcile(&[WatchedPath::recursive("/denied")], filter([]));
		let errors = drain(&mut recursor);

		assert_eq!(errors.len(), 2);
		assert_eq!(watched(&backend, "/denied"), 0);
		assert_eq!(watched(&backend, "/"), 1);
		assert_eq!(
			recursor.root_guards.values().next(),
			Some(&PathBuf::from("/"))
		);
		assert_eq!(
			scanner
				.lock()
				.unwrap()
				.operations
				.iter()
				.filter(|operation| {
					matches!(operation, Operation::Classify(path) if path == Path::new("/denied"))
				})
				.count(),
			2
		);

		scanner
			.lock()
			.unwrap()
			.classify_errors
			.remove(Path::new("/denied"));
		directory(&scanner, "/denied");
		recursor.topology_create("/denied".into());
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/denied"), 1);
		assert!(recursor.root_guards.is_empty());
	}

	#[test]
	fn non_followed_explicit_symlink_uses_only_ancestor_guard() {
		let (mut recursor, backend, scanner) = fixture();
		recursor.follow_symlinks = false;
		scanner
			.lock()
			.unwrap()
			.non_followed_symlinks
			.insert("/link".into());

		recursor.reconcile(&[WatchedPath::recursive("/link")], filter([]));
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/link"), 0);
		assert_eq!(watched(&backend, "/"), 1);
		assert_eq!(
			recursor.root_guards.values().next(),
			Some(&PathBuf::from("/"))
		);
	}

	#[test]
	fn non_followed_component_uses_nearest_safe_guard() {
		let (mut recursor, backend, scanner) = fixture();
		recursor.follow_symlinks = false;
		directory(&scanner, "/base");
		scanner
			.lock()
			.unwrap()
			.non_followed_symlinks
			.insert("/base/link".into());

		recursor.reconcile(
			&[WatchedPath::recursive("/base/link/deeper/root")],
			filter([]),
		);
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/base"), 1);
		assert_eq!(watched(&backend, "/base/link"), 0);
		assert_eq!(watched(&backend, "/base/link/deeper/root"), 0);

		scanner
			.lock()
			.unwrap()
			.non_followed_symlinks
			.remove(Path::new("/base/link"));
		for path in ["/base/link", "/base/link/deeper", "/base/link/deeper/root"] {
			directory(&scanner, path);
		}
		recursor.topology_create("/base/link".into());
		drain(&mut recursor);
		assert_eq!(watched(&backend, "/base/link/deeper/root"), 1);
		assert!(recursor.root_guards.is_empty());
	}

	#[test]
	fn unmanaged_non_followed_component_skips_notify_registration() {
		let backend = Arc::new(Mutex::new(FakeBackendState::default()));
		let scanner = Arc::new(Mutex::new(FakeScannerState::default()));
		directory(&scanner, "/");
		directory(&scanner, "/base");
		scanner
			.lock()
			.unwrap()
			.non_followed_symlinks
			.insert("/base/link".into());
		let mut recursor = Recursor::new(
			Box::new(FakeBackend(backend.clone())),
			Box::new(FakeScanner(scanner)),
			Watcher::Native,
			notify::WatcherKind::Fsevent,
			false,
			PathBuf::from("/work"),
			filter([]),
		);

		recursor.reconcile(
			&[WatchedPath::recursive("/base/link/deeper/root")],
			filter([]),
		);
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/base/link/deeper/root"), 0);
		assert_eq!(watched(&backend, "/base"), 0);
	}

	#[test]
	fn followed_missing_root_guards_lexical_symlink_ancestor() {
		let (mut recursor, backend, scanner) = fixture();
		scanner
			.lock()
			.unwrap()
			.directories
			.insert("/alias".into(), "/target".into());
		scanner
			.lock()
			.unwrap()
			.not_found
			.insert("/alias/missing".into());

		recursor.reconcile(&[WatchedPath::recursive("/alias/missing")], filter([]));
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/alias"), 1);
		assert_eq!(watched(&backend, "/"), 0);
		assert_eq!(
			recursor.root_guards.values().next(),
			Some(&PathBuf::from("/alias"))
		);
	}

	#[test]
	fn canonical_source_alias_covers_guard_and_projects_topology() {
		let (mut recursor, backend, scanner) = fixture();
		scanner.lock().unwrap().directories.extend([
			(PathBuf::from("/a-source"), PathBuf::from("/real")),
			(PathBuf::from("/b-alias"), PathBuf::from("/real")),
		]);
		scanner
			.lock()
			.unwrap()
			.not_found
			.insert("/b-alias/missing".into());
		recursor.reconcile(
			&[
				WatchedPath::recursive("/a-source"),
				WatchedPath::recursive("/b-alias/missing"),
			],
			filter([]),
		);
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/a-source"), 1);
		assert_eq!(watched(&backend, "/b-alias"), 0);
		assert_eq!(
			recursor
				.root_guards
				.values()
				.find(|path| path.as_path() == Path::new("/b-alias")),
			Some(&PathBuf::from("/b-alias"))
		);

		scanner
			.lock()
			.unwrap()
			.not_found
			.remove(Path::new("/b-alias/missing"));
		scanner.lock().unwrap().directories.extend([
			(
				PathBuf::from("/a-source/missing"),
				PathBuf::from("/real/missing"),
			),
			(
				PathBuf::from("/b-alias/missing"),
				PathBuf::from("/real/missing"),
			),
		]);
		// The backend is registered at A, so its lexical event must also drive
		// the guarded B alias.
		recursor.topology_create("/a-source/missing".into());
		drain(&mut recursor);

		assert!(recursor.logical.contains_key(Path::new("/a-source")));
		assert!(recursor.logical.contains_key(Path::new("/b-alias/missing")));
		assert_eq!(
			watched(&backend, "/a-source/missing") + watched(&backend, "/b-alias/missing"),
			1
		);
		assert_eq!(unwatched(&backend, "/a-source"), 0);
		assert_eq!(watched(&backend, "/a-source"), 1);
		assert!(recursor.root_guards.is_empty());
	}

	#[test]
	fn configured_file_root_is_reacquired_after_delete_recreate() {
		let (mut recursor, backend, scanner) = fixture();
		recursor.reconcile(&[WatchedPath::non_recursive("/file")], filter([]));
		drain(&mut recursor);

		scanner.lock().unwrap().not_found.insert("/file".into());
		recursor.topology_remove("/file".into());
		drain(&mut recursor);
		scanner.lock().unwrap().not_found.remove(Path::new("/file"));
		recursor.topology_create("/file".into());
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/file"), 2);
		assert!(recursor.logical.contains_key(Path::new("/file")));
	}

	#[test]
	fn parent_guard_events_outside_roots_are_not_public() {
		let (mut recursor, _backend, scanner) = fixture();
		directory(&scanner, "/root");
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		let sibling = notify::Event::new(notify::EventKind::Create(notify::event::CreateKind::Any))
			.add_path("/sibling".into());
		let root = notify::Event::new(notify::EventKind::Create(notify::event::CreateKind::Any))
			.add_path("/root".into());
		assert!(!recursor.event_is_public(&sibling));
		assert!(recursor.event_is_public(&root));
	}

	#[test]
	fn create_scans_an_already_populated_subtree() {
		let (mut recursor, backend, scanner) = fixture();
		directory(&scanner, "/root");
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		for path in ["/root/new", "/root/new/child"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root/new", &["/root/new/child"]);
		recursor.topology_create("/root/new".into());
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/root/new"), 1);
		assert_eq!(watched(&backend, "/root/new/child"), 1);
	}

	#[test]
	fn split_replacement_rebuilds_populated_subtree_and_later_nested_state() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/item", "/root/item/old"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/item"]);
		entries(&scanner, "/root/item", &["/root/item/old"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		scanner
			.lock()
			.unwrap()
			.not_found
			.insert("/root/item/old".into());
		for path in ["/root/item/new", "/root/item/new/grand"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root/item", &["/root/item/new"]);
		entries(&scanner, "/root/item/new", &[]);

		// Split From(P)/To(P) delivery must not leave From's subtree tombstone
		// suppressing the authoritative replacement at P.
		recursor.topology_remove("/root/item".into());
		recursor.topology_ambiguous("/root/item".into());
		drain(&mut recursor);

		assert!(!recursor.logical.contains_key(Path::new("/root/item/old")));
		assert!(recursor.logical.contains_key(Path::new("/root/item/new")));
		assert_eq!(watched(&backend, "/root/item"), 2);

		entries(&scanner, "/root/item/new", &["/root/item/new/grand"]);
		recursor.topology_create("/root/item/new/grand".into());
		drain(&mut recursor);

		assert!(recursor
			.logical
			.contains_key(Path::new("/root/item/new/grand")));
		assert!(recursor.event_is_public(
			&notify::Event::new(notify::EventKind::Modify(notify::event::ModifyKind::Any))
				.add_path("/root/item/new/grand".into())
		));
	}

	#[test]
	fn topology_does_not_inherit_stale_owner_during_filter_epoch() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/ignored", "/root/ignored/new"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/ignored"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		recursor.reconcile(
			&[WatchedPath::recursive("/root")],
			filter(["/root/ignored"]),
		);
		// The ignored directory still has old-epoch ownership here, but must not
		// be used as the parent of a topology addition.
		recursor.topology_create("/root/ignored/new".into());
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/root/ignored/new"), 0);
		assert!(!recursor
			.logical
			.contains_key(Path::new("/root/ignored/new")));
	}

	#[test]
	fn filter_replacement_prunes_and_reopens_subtrees() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/build", "/root/build/cache"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/build"]);
		entries(&scanner, "/root/build", &["/root/build/cache"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		recursor.reconcile(&[WatchedPath::recursive("/root")], filter(["/root/build"]));
		drain(&mut recursor);
		assert_eq!(unwatched(&backend, "/root/build/cache"), 1);
		assert_eq!(unwatched(&backend, "/root/build"), 1);

		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);
		assert_eq!(watched(&backend, "/root/build"), 2);
		assert_eq!(watched(&backend, "/root/build/cache"), 2);
	}

	#[test]
	fn resource_error_preserves_existing_watches_and_latches_epoch() {
		let (mut recursor, backend, scanner) = fixture();
		directory(&scanner, "/root");
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);
		assert_eq!(watched(&backend, "/root"), 1);

		directory(&scanner, "/root/new");
		backend.lock().unwrap().resource_failure = Some("/root/new".into());
		recursor.topology_create("/root/new".into());
		let first = recursor.step();
		assert!(!first.rebuild_backend);
		assert_eq!(first.errors.len(), 1);
		assert!(matches!(
			first.errors[0],
			RuntimeError::FsWatcher {
				err: FsWatcherError::TooManyWatches(_),
				..
			}
		));
		assert!(recursor.logical.contains_key(Path::new("/root")));
		assert_eq!(unwatched(&backend, "/root"), 0);

		// Further additions are skipped without another backend call or error.
		directory(&scanner, "/root/other");
		recursor.topology_create("/root/other".into());
		let second = recursor.step();
		assert!(second.errors.is_empty());
		assert_eq!(watched(&backend, "/root/other"), 0);

		// A relevant reconciliation starts a fresh epoch and retries additions.
		backend.lock().unwrap().resource_failure = None;
		entries(&scanner, "/root", &["/root/new", "/root/other"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);
		assert_eq!(watched(&backend, "/root/new"), 2);
		assert_eq!(watched(&backend, "/root/other"), 1);
	}

	#[test]
	fn resource_skipped_candidate_retries_when_removal_frees_capacity() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/old", "/root/new"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/old"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		backend.lock().unwrap().resource_failure = Some("/root/new".into());
		recursor.topology_create("/root/new".into());
		let failed = recursor.step();
		assert_eq!(failed.errors.len(), 1);
		assert_eq!(watched(&backend, "/root/new"), 1);

		backend.lock().unwrap().resource_failure = None;
		recursor.topology_remove("/root/old".into());
		drain(&mut recursor);

		assert_eq!(unwatched(&backend, "/root/old"), 1);
		assert_eq!(watched(&backend, "/root/new"), 2);
		assert!(recursor.logical.contains_key(Path::new("/root/new")));
	}

	#[test]
	fn generic_watch_failure_is_retried_once_on_fresh_backend() {
		let (mut recursor, backend, scanner) = fixture();
		directory(&scanner, "/root");
		backend
			.lock()
			.unwrap()
			.generic_failures
			.insert("/root".into(), 1);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));

		let errors = run_until_rebuild(&mut recursor);
		assert_eq!(errors.len(), 1);
		recursor.replace_backend(Box::new(FakeBackend(backend.clone())));
		assert!(drain(&mut recursor).is_empty());
		assert_eq!(watched(&backend, "/root"), 2);
		assert!(recursor.logical.contains_key(Path::new("/root")));
	}

	#[test]
	fn backend_path_not_found_is_local_invalidation_not_rebuild() {
		let (mut recursor, backend, scanner) = fixture();
		directory(&scanner, "/root");
		backend
			.lock()
			.unwrap()
			.path_not_found_failures
			.insert("/root".into(), 1);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));

		let errors = drain(&mut recursor);

		assert_eq!(errors.len(), 1);
		assert_eq!(watched(&backend, "/root"), 2);
		assert!(recursor.logical.contains_key(Path::new("/root")));
		assert!(recursor.addition_failures.get(Path::new("/root")).is_none());
		assert!(!recursor.skipped_additions.contains(Path::new("/root")));
	}

	#[test]
	fn recreated_descendant_clears_persistent_watch_suppression() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/child"] {
			directory(&scanner, path);
		}
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);
		backend
			.lock()
			.unwrap()
			.generic_failures
			.insert("/root/child".into(), 2);

		recursor.topology_create("/root/child".into());
		assert_eq!(run_until_rebuild(&mut recursor).len(), 1);
		recursor.replace_backend(Box::new(FakeBackend(backend.clone())));
		assert_eq!(drain(&mut recursor).len(), 1);
		assert!(recursor
			.skipped_additions
			.contains(Path::new("/root/child")));
		assert!(!recursor.logical.contains_key(Path::new("/root/child")));

		// A Create for a new inode/object is authoritative and must clear the
		// old path's permanent suppression without waiting for reconfiguration.
		recursor.topology_create("/root/child".into());
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/root/child"), 3);
		assert!(!recursor
			.skipped_additions
			.contains(Path::new("/root/child")));
		assert!(recursor.logical.contains_key(Path::new("/root/child")));
	}

	#[test]
	fn generic_rebuild_replays_coverage_before_scanner_failures() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/child", "/root/child/grand", "/root/new"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/child"]);
		entries(&scanner, "/root/child", &["/root/child/grand"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		backend
			.lock()
			.unwrap()
			.generic_failures
			.insert("/root/new".into(), 1);
		recursor.topology_create("/root/new".into());
		assert_eq!(run_until_rebuild(&mut recursor).len(), 1);
		scanner
			.lock()
			.unwrap()
			.classify_errors
			.insert("/root/child".into());

		let replacement = Arc::new(Mutex::new(FakeBackendState::default()));
		recursor.replace_backend(Box::new(FakeBackend(replacement.clone())));
		drain(&mut recursor);
		assert_eq!(watched(&replacement, "/root/child"), 1);
		assert_eq!(watched(&replacement, "/root/child/grand"), 1);
		assert_eq!(watched(&replacement, "/root/new"), 1);
		assert!(recursor
			.logical
			.contains_key(Path::new("/root/child/grand")));
	}

	#[test]
	fn persistent_watch_failure_is_excluded_after_one_retry() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/bad", "/healthy"] {
			directory(&scanner, path);
		}
		recursor.reconcile(&[WatchedPath::recursive("/healthy")], filter([]));
		drain(&mut recursor);
		backend
			.lock()
			.unwrap()
			.generic_failures
			.insert("/bad".into(), 2);
		recursor.reconcile(
			&[
				WatchedPath::recursive("/bad"),
				WatchedPath::recursive("/healthy"),
			],
			filter([]),
		);

		assert_eq!(run_until_rebuild(&mut recursor).len(), 1);
		recursor.replace_backend(Box::new(FakeBackend(backend.clone())));
		assert_eq!(drain(&mut recursor).len(), 1);

		assert_eq!(watched(&backend, "/bad"), 2);
		assert_eq!(watched(&backend, "/healthy"), 2);
		assert!(!recursor.logical.contains_key(Path::new("/bad")));
		assert!(recursor.logical.contains_key(Path::new("/healthy")));
	}

	#[test]
	fn replacing_backend_replays_all_roots() {
		let (mut recursor, _backend, scanner) = fixture();
		directory(&scanner, "/root");
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		let replacement = Arc::new(Mutex::new(FakeBackendState::default()));
		recursor.replace_backend(Box::new(FakeBackend(replacement.clone())));
		drain(&mut recursor);

		assert_eq!(watched(&replacement, "/root"), 1);
	}

	#[test]
	fn fresh_backend_replays_known_coverage_before_returning() {
		let (mut recursor, _backend, scanner) = fixture();
		for path in ["/root", "/root/child"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/child"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		let replacement = Arc::new(Mutex::new(FakeBackendState::default()));
		recursor.prepare_backend_rebuild();
		recursor.install_backend(Box::new(FakeBackend(replacement.clone())));
		let replay = recursor.replay_backend_snapshot();

		assert!(!replay.rebuild_backend);
		assert!(replay.errors.is_empty());
		assert_eq!(watched(&replacement, "/root"), 1);
		assert_eq!(watched(&replacement, "/root/child"), 1);
		assert!(recursor.replay_queue.is_empty());
		assert!(recursor.logical.contains_key(Path::new("/root/child")));
	}

	#[test]
	fn replay_path_not_found_migrates_to_surviving_alias() {
		let (mut recursor, _backend, scanner) = fixture();
		scanner.lock().unwrap().directories.extend([
			(PathBuf::from("/alias-a"), PathBuf::from("/real")),
			(PathBuf::from("/alias-b"), PathBuf::from("/real")),
		]);
		recursor.reconcile(
			&[
				WatchedPath::recursive("/alias-a"),
				WatchedPath::recursive("/alias-b"),
			],
			filter([]),
		);
		drain(&mut recursor);

		let replacement = Arc::new(Mutex::new(FakeBackendState::default()));
		replacement
			.lock()
			.unwrap()
			.path_not_found_failures
			.insert("/alias-a".into(), 1);
		recursor.prepare_backend_rebuild();
		recursor.install_backend(Box::new(FakeBackend(replacement.clone())));
		let replay = recursor.replay_backend_snapshot();

		assert_eq!(replay.errors.len(), 1);
		assert!(!replay.rebuild_backend);
		assert_eq!(watched(&replacement, "/alias-a"), 1);
		assert_eq!(watched(&replacement, "/alias-b"), 1);
		assert!(recursor.replay_queue.is_empty());
		assert!(recursor.replay_desired.is_empty());
		assert!(!recursor.logical.contains_key(Path::new("/alias-a")));
		assert!(recursor.logical.contains_key(Path::new("/alias-b")));
		assert_eq!(
			recursor
				.physical
				.get(&Identity::Canonical("/real".into()))
				.unwrap()
				.watch_path,
			PathBuf::from("/alias-b")
		);
	}

	#[test]
	fn replay_resource_latch_stops_later_calls_and_retains_pending() {
		let (mut recursor, _backend, scanner) = fixture();
		for path in ["/a", "/b"] {
			directory(&scanner, path);
		}
		recursor.reconcile(
			&[WatchedPath::recursive("/a"), WatchedPath::recursive("/b")],
			filter([]),
		);
		drain(&mut recursor);

		let replacement = Arc::new(Mutex::new(FakeBackendState::default()));
		replacement.lock().unwrap().resource_failure = Some("/a".into());
		recursor.prepare_backend_rebuild();
		recursor.install_backend(Box::new(FakeBackend(replacement.clone())));
		let replay = recursor.replay_backend_snapshot();

		assert_eq!(replay.errors.len(), 1);
		assert!(!replay.rebuild_backend);
		assert_eq!(watched(&replacement, "/a"), 1);
		assert_eq!(watched(&replacement, "/b"), 0);
		assert_eq!(recursor.replay_desired.len(), 2);
		assert!(recursor.replay_queue.is_empty());
		assert!(recursor.needs_retry());
	}

	#[test]
	fn managed_rescan_rebuilds_and_replays_backend() {
		let (mut recursor, _backend, scanner) = fixture();
		directory(&scanner, "/root");
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		recursor.rescan();
		assert!(run_until_rebuild(&mut recursor).is_empty());
		let replacement = Arc::new(Mutex::new(FakeBackendState::default()));
		recursor.replace_backend(Box::new(FakeBackend(replacement.clone())));
		drain(&mut recursor);

		assert_eq!(watched(&replacement, "/root"), 1);
		assert!(recursor.logical.contains_key(Path::new("/root")));
	}

	#[test]
	fn canonical_identity_change_rebuilds_before_scanning_new_tree() {
		let (mut recursor, backend, scanner) = fixture();
		directory(&scanner, "/root");
		scanner
			.lock()
			.unwrap()
			.directories
			.insert("/root/link".into(), "/identity/one".into());
		entries(&scanner, "/root", &["/root/link"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		scanner
			.lock()
			.unwrap()
			.directories
			.insert("/root/link".into(), "/identity/two".into());
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		assert!(run_until_rebuild(&mut recursor).is_empty());

		let replacement = Arc::new(Mutex::new(FakeBackendState::default()));
		recursor.replace_backend(Box::new(FakeBackend(replacement)));
		drain(&mut recursor);
		assert_eq!(
			recursor
				.logical
				.get(Path::new("/root/link"))
				.unwrap()
				.identity,
			Identity::Canonical("/identity/two".into())
		);
		assert_eq!(watched(&backend, "/root/link"), 1);
	}

	#[test]
	fn paired_rename_invalidates_existing_destination_before_add() {
		let (mut recursor, backend, scanner) = fixture();
		for path in [
			"/root",
			"/root/from",
			"/root/to",
			"/root/to/old",
			"/root/to/new",
		] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/from", "/root/to"]);
		entries(&scanner, "/root/to", &["/root/to/old"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		entries(&scanner, "/root/to", &["/root/to/new"]);
		recursor.topology_rename("/root/from".into(), "/root/to".into());
		drain(&mut recursor);

		assert_eq!(unwatched(&backend, "/root/to/old"), 1);
		assert_eq!(unwatched(&backend, "/root/to"), 1);
		assert_eq!(watched(&backend, "/root/to"), 2);
		assert_eq!(watched(&backend, "/root/to/new"), 1);
		assert!(!recursor.logical.contains_key(Path::new("/root/from")));
	}

	#[test]
	fn ambiguous_directory_to_file_removes_stale_subtree() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/root", "/root/item", "/root/item/child"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/root", &["/root/item"]);
		entries(&scanner, "/root/item", &["/root/item/child"]);
		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		scanner
			.lock()
			.unwrap()
			.directories
			.remove(Path::new("/root/item"));
		recursor.topology_ambiguous("/root/item".into());
		drain(&mut recursor);

		assert!(!recursor.logical.contains_key(Path::new("/root/item")));
		assert!(!recursor.logical.contains_key(Path::new("/root/item/child")));
		assert_eq!(unwatched(&backend, "/root/item/child"), 1);
		assert_eq!(unwatched(&backend, "/root/item"), 1);
	}

	#[test]
	fn ambiguous_same_identity_directory_is_reregistered() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/item", "/item/child"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/item", &["/item/child"]);
		recursor.reconcile(&[WatchedPath::recursive("/item")], filter([]));
		drain(&mut recursor);

		recursor.topology_ambiguous("/item".into());
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/item"), 2);
		assert_eq!(watched(&backend, "/item/child"), 2);
	}

	#[test]
	fn same_path_root_replacement_installs_new_identity() {
		let (mut recursor, backend, scanner) = fixture();
		for path in ["/item", "/item/child"] {
			directory(&scanner, path);
		}
		entries(&scanner, "/item", &["/item/child"]);
		recursor.reconcile(&[WatchedPath::recursive("/item")], filter([]));
		drain(&mut recursor);

		scanner
			.lock()
			.unwrap()
			.directories
			.remove(Path::new("/item"));
		recursor.topology_ambiguous("/item".into());
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/item"), 2);
		assert!(!recursor.logical.contains_key(Path::new("/item/child")));
		assert_eq!(
			recursor.logical.get(Path::new("/item")).unwrap().identity,
			Identity::Lexical("/item".into())
		);
	}

	#[test]
	fn removing_representative_alias_replays_surviving_alias() {
		let (mut recursor, backend, scanner) = fixture();
		scanner.lock().unwrap().directories.extend([
			(PathBuf::from("/alias1"), PathBuf::from("/real")),
			(PathBuf::from("/alias2"), PathBuf::from("/real")),
		]);
		recursor.reconcile(
			&[
				WatchedPath::recursive("/alias1"),
				WatchedPath::recursive("/alias2"),
			],
			filter([]),
		);
		drain(&mut recursor);
		assert_eq!(watched(&backend, "/alias1"), 1);
		assert_eq!(watched(&backend, "/alias2"), 0);

		recursor.reconcile(&[WatchedPath::recursive("/alias2")], filter([]));
		assert!(run_until_rebuild(&mut recursor).is_empty());
		let replacement = Arc::new(Mutex::new(FakeBackendState::default()));
		recursor.replace_backend(Box::new(FakeBackend(replacement.clone())));
		drain(&mut recursor);
		assert_eq!(watched(&replacement, "/alias2"), 1);

		scanner
			.lock()
			.unwrap()
			.directories
			.insert("/alias2/new".into(), "/real/new".into());
		recursor.topology_create("/alias2/new".into());
		drain(&mut recursor);
		assert_eq!(watched(&replacement, "/alias2/new"), 1);
	}

	#[test]
	fn canonical_identity_breaks_basic_symlink_cycle() {
		let (mut recursor, backend, scanner) = fixture();
		directory(&scanner, "/root");
		scanner
			.lock()
			.unwrap()
			.directories
			.insert("/root/link".into(), "/root".into());
		entries(&scanner, "/root", &["/root/link"]);
		entries(&scanner, "/root/link", &["/root/link"]);

		recursor.reconcile(&[WatchedPath::recursive("/root")], filter([]));
		drain(&mut recursor);

		assert_eq!(watched(&backend, "/root"), 1);
		assert_eq!(watched(&backend, "/root/link"), 0);
		assert!(!scanner
			.lock()
			.unwrap()
			.operations
			.contains(&Operation::Scan("/root/link".into())));
	}
}
