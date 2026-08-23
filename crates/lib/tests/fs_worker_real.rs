use std::{
	path::{Path, PathBuf},
	sync::Arc,
	time::Duration,
};

#[cfg(target_os = "linux")]
use std::fmt;

use async_priority_channel as priority;
use notify::EventKind;
use tempfile::TempDir;
use tokio::{
	sync::{mpsc, watch},
	task::JoinHandle,
	time::{timeout, Instant},
};
#[cfg(unix)]
use watchexec::error::FsWatcherError;
use watchexec::{
	error::{CriticalError, RuntimeError},
	filter::Filterer,
	sources::fs::{worker, Watcher},
	Config, WatchedPath,
};
use watchexec_events::{Event, Priority, Tag};

const TEST_TIMEOUT: Duration = Duration::from_secs(5);
const QUIET_TIMEOUT: Duration = Duration::from_millis(300);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug)]
struct WatcherCase {
	name: &'static str,
	watcher: Watcher,
}

fn managed_watcher_cases() -> Vec<WatcherCase> {
	let poll = WatcherCase {
		name: "poll",
		watcher: Watcher::Poll(POLL_INTERVAL),
	};

	// These native backends have independent non-recursive registrations and
	// therefore use watchexec's source-filtering recursor.
	#[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
	{
		vec![
			WatcherCase {
				name: "native",
				watcher: Watcher::Native,
			},
			poll,
		]
	}

	#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "windows")))]
	{
		vec![poll]
	}
}

#[derive(Clone, Debug, Default)]
struct DirFilter {
	denied: Vec<PathBuf>,
}

impl DirFilter {
	fn denying(paths: impl IntoIterator<Item = PathBuf>) -> Self {
		Self {
			denied: paths.into_iter().collect(),
		}
	}
}

impl Filterer for DirFilter {
	fn check_dir(&self, path: &Path) -> Result<bool, RuntimeError> {
		Ok(!self
			.denied
			.iter()
			.any(|denied| path == denied || path.starts_with(denied)))
	}

	fn check_event(&self, _event: &Event, _priority: Priority) -> Result<bool, RuntimeError> {
		Ok(true)
	}
}

struct FsHarness {
	case: WatcherCase,
	config: Arc<Config>,
	ready: watch::Receiver<()>,
	events: priority::Receiver<Event, Priority>,
	errors: mpsc::Receiver<RuntimeError>,
	task: Option<JoinHandle<Result<(), CriticalError>>>,
}

impl FsHarness {
	async fn start(
		case: WatcherCase,
		paths: Vec<WatchedPath>,
		follow_symlinks: bool,
		filterer: impl Filterer + 'static,
	) -> Self {
		let config = Arc::new(Config::default());
		config.file_watcher(case.watcher);
		config.follow_symlinks(follow_symlinks);
		config.filterer(filterer);
		config.pathset(paths);

		// Subscribe after configuring but before spawning: the worker has not yet
		// emitted the initial readiness notification, so it cannot be missed.
		let ready = config.fs_ready();
		let event_capacity =
			u64::try_from(config.event_channel_size).expect("event channel size does not fit u64");
		let (event_tx, events) = priority::bounded(event_capacity);
		let (error_tx, errors) = mpsc::channel(config.error_channel_size);
		let task = tokio::spawn(worker(config.clone(), error_tx, event_tx));

		let mut harness = Self {
			case,
			config,
			ready,
			events,
			errors,
			task: Some(task),
		};
		harness.wait_ready("initial filesystem configuration").await;
		harness
	}

	async fn wait_ready(&mut self, operation: &str) {
		match timeout(TEST_TIMEOUT, self.ready.changed()).await {
			Ok(Ok(())) => {}
			Ok(Err(error)) => panic!(
				"{} watcher readiness channel closed while waiting for {operation}: {error}",
				self.case.name
			),
			Err(elapsed) => panic!(
				"{} watcher did not become ready within {TEST_TIMEOUT:?} while waiting for {operation}: {elapsed}; worker_finished={}",
				self.case.name,
				self.task_finished()
			),
		}
	}

	async fn set_paths(&mut self, paths: Vec<WatchedPath>) {
		self.config.pathset(paths);
		self.wait_ready("pathset replacement").await;
		self.drain_events();
	}

	async fn set_filterer(&mut self, filterer: impl Filterer + 'static) {
		self.config.filterer(filterer);
		self.wait_ready("filterer replacement").await;
		self.drain_events();
	}

	fn drain_events(&self) {
		while self.events.try_recv().is_ok() {}
	}

	async fn drain_until_quiet(&self) {
		while matches!(timeout(QUIET_TIMEOUT, self.events.recv()).await, Ok(Ok(_))) {}
	}

	fn task_finished(&self) -> bool {
		self.task.as_ref().map_or(true, JoinHandle::is_finished)
	}

	fn take_available_errors(&mut self) -> Vec<String> {
		let mut errors = Vec::new();
		while let Ok(error) = self.errors.try_recv() {
			errors.push(format!("{error:?}"));
		}
		errors
	}

	async fn wait_for_any_path(&mut self, expected: &[PathBuf], operation: &str) -> Event {
		self.wait_for_any_path_matching(expected, |_| true, operation)
			.await
	}

	async fn wait_for_any_path_matching(
		&mut self,
		expected: &[PathBuf],
		matches_event: impl Fn(&Event) -> bool,
		operation: &str,
	) -> Event {
		let deadline = Instant::now() + TEST_TIMEOUT;
		let mut observed = Vec::new();

		loop {
			let now = Instant::now();
			if now >= deadline {
				let errors = self.take_available_errors();
				panic!(
					"{} watcher did not report a matching event for any of {expected:?} while {operation}; observed={observed:?}; runtime_errors={errors:?}; worker_finished={}",
					self.case.name,
					self.task_finished()
				);
			}

			match timeout(deadline.saturating_duration_since(now), self.events.recv()).await {
				Ok(Ok((event, _priority))) => {
					let paths = event_paths(&event);
					if matches_event(&event) && paths.iter().any(|path| expected.contains(path)) {
						return event;
					}
					observed.push(format!("{event:?}"));
				}
				Ok(Err(error)) => panic!(
					"{} watcher event channel closed while {operation}: {error:?}",
					self.case.name
				),
				Err(_) => {
					let errors = self.take_available_errors();
					panic!(
						"{} watcher timed out while {operation}; expected={expected:?}; observed={observed:?}; runtime_errors={errors:?}; worker_finished={}",
						self.case.name,
						self.task_finished()
					);
				}
			}
		}
	}

	async fn assert_no_path_under(&self, forbidden: &[PathBuf], operation: &str) {
		let deadline = Instant::now() + QUIET_TIMEOUT;
		let mut observed = Vec::new();

		loop {
			let now = Instant::now();
			if now >= deadline {
				return;
			}

			match timeout(deadline.saturating_duration_since(now), self.events.recv()).await {
				Ok(Ok((event, _priority))) => {
					let paths = event_paths(&event);
					// A watched parent can report the forbidden directory itself even
					// when no watch is installed inside that directory.
					assert!(
						!paths.iter().any(|path| {
							forbidden
								.iter()
								.any(|prefix| path != prefix && path.starts_with(prefix))
						}),
						"{} watcher unexpectedly reported a path under {forbidden:?} while {operation}: event_paths={paths:?}; previously_observed={observed:?}",
						self.case.name
					);
					observed.push(paths);
				}
				Ok(Err(error)) => panic!(
					"{} watcher event channel closed while checking {operation}: {error:?}",
					self.case.name
				),
				Err(_) => return,
			}
		}
	}

	async fn shutdown(mut self) {
		let task = self.task.take().expect("filesystem worker task missing");
		if task.is_finished() {
			match task.await {
				Ok(Ok(())) => panic!("{} filesystem worker exited unexpectedly", self.case.name),
				Ok(Err(error)) => panic!(
					"{} filesystem worker failed unexpectedly: {error:?}",
					self.case.name
				),
				Err(error) => panic!(
					"{} filesystem worker task failed unexpectedly: {error:?}",
					self.case.name
				),
			}
		} else {
			task.abort();
			let result = task.await;
			assert!(
				matches!(result, Err(ref error) if error.is_cancelled()),
				"{} filesystem worker did not cancel cleanly: {result:?}",
				self.case.name
			);
		}
	}
}

impl Drop for FsHarness {
	fn drop(&mut self) {
		if let Some(task) = self.task.take() {
			task.abort();
		}
	}
}

fn event_paths(event: &Event) -> Vec<PathBuf> {
	event.paths().map(|(path, _)| path.to_owned()).collect()
}

fn event_is_create(event: &Event) -> bool {
	event
		.tags
		.iter()
		.any(|tag| matches!(tag, Tag::FileEventKind(EventKind::Create(_))))
}

fn event_is_remove(event: &Event) -> bool {
	event
		.tags
		.iter()
		.any(|tag| matches!(tag, Tag::FileEventKind(EventKind::Remove(_))))
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
fn event_is_non_name_modify(event: &Event) -> bool {
	event.tags.iter().any(|tag| {
		matches!(
			tag,
			Tag::FileEventKind(EventKind::Modify(kind))
				if !matches!(kind, notify::event::ModifyKind::Name(_))
		)
	})
}

fn make_tempdir(case: WatcherCase, test: &str) -> TempDir {
	tempfile::Builder::new()
		.prefix(&format!("watchexec-fs-{test}-{}-", case.name))
		.tempdir()
		.unwrap_or_else(|error| panic!("failed to create temporary directory: {error}"))
}

fn create_dir(path: &Path) {
	std::fs::create_dir_all(path)
		.unwrap_or_else(|error| panic!("failed to create directory {}: {error}", path.display()));
}

fn write_file(path: &Path, contents: &str) {
	std::fs::write(path, contents)
		.unwrap_or_else(|error| panic!("failed to write file {}: {error}", path.display()));
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
fn replace_file(path: &Path, replacement: &Path, contents: &str) {
	write_file(replacement, contents);
	#[cfg(target_os = "windows")]
	std::fs::remove_file(path)
		.unwrap_or_else(|error| panic!("failed to remove file {}: {error}", path.display()));
	std::fs::rename(replacement, path).unwrap_or_else(|error| {
		panic!(
			"failed to replace {} with {}: {error}",
			path.display(),
			replacement.display()
		)
	});
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recursive_filter_physically_prunes_ignored_subtree() {
	for case in managed_watcher_cases() {
		let temp = make_tempdir(case, "prune");
		let root = temp.path().join("root");
		let accepted = root.join("src");
		let accepted_nested = accepted.join("nested");
		let ignored = root.join("target");
		let ignored_nested = ignored.join("nested");
		create_dir(&accepted_nested);
		create_dir(&ignored_nested);

		let mut harness = FsHarness::start(
			case,
			vec![WatchedPath::recursive(&root)],
			true,
			DirFilter::denying([ignored.clone()]),
		)
		.await;

		#[cfg(target_os = "linux")]
		if case.watcher == Watcher::Native {
			assert_linux_inotify_registration(&root, &accepted, true);
			assert_linux_inotify_registration(&root, &accepted_nested, true);
			assert_linux_inotify_registration(&root, &ignored, false);
			assert_linux_inotify_registration(&root, &ignored_nested, false);
		}

		let ignored_file = ignored_nested.join("ignored.txt");
		let accepted_file = accepted_nested.join("accepted.txt");
		write_file(&ignored_file, "ignored");
		write_file(&accepted_file, "accepted");
		harness
			.wait_for_any_path(&[accepted_file], "waiting for an accepted sibling change")
			.await;
		harness
			.assert_no_path_under(&[ignored], "checking the source-pruned subtree")
			.await;
		harness.shutdown().await;
	}
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn directories_created_or_moved_in_after_readiness_are_covered() {
	for case in managed_watcher_cases() {
		let temp = make_tempdir(case, "new-tree");
		let root = temp.path().join("root");
		create_dir(&root);
		let mut harness =
			FsHarness::start(case, vec![WatchedPath::recursive(&root)], true, ()).await;

		let new_tree = root.join("created-after-ready");
		let new_deep = new_tree.join("one/two");
		let seed = new_deep.join("seed.txt");
		create_dir(&new_deep);
		write_file(&seed, "already populated");
		harness
			.wait_for_any_path(
				&[new_tree.clone(), new_deep.clone(), seed],
				"discovering a newly populated directory",
			)
			.await;

		let probe = new_deep.join("probe.txt");
		write_file(&probe, "probe");
		harness
			.wait_for_any_path(&[probe], "checking the newly discovered deep directory")
			.await;

		#[cfg(not(target_os = "windows"))]
		{
			let staging = temp.path().join("staging");
			let staging_deep = staging.join("full/subtree");
			create_dir(&staging_deep);
			write_file(&staging_deep.join("seed.txt"), "moved seed");
			let moved = root.join("moved-in");
			std::fs::rename(&staging, &moved).unwrap_or_else(|error| {
				panic!("failed to move populated directory {staging:?} to {moved:?}: {error}")
			});
			let moved_deep = moved.join("full/subtree");
			harness
				.wait_for_any_path(
					&[
						moved.clone(),
						moved_deep.clone(),
						moved_deep.join("seed.txt"),
					],
					"discovering a moved-in populated directory",
				)
				.await;

			let moved_probe = moved_deep.join("probe.txt");
			write_file(&moved_probe, "probe");
			harness
				.wait_for_any_path(&[moved_probe], "checking the moved-in deep directory")
				.await;
		}

		harness.shutdown().await;
	}
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn replacing_filterer_live_prunes_and_discovers_existing_subtrees() {
	for case in managed_watcher_cases() {
		let temp = make_tempdir(case, "replace-filter");
		let root = temp.path().join("root");
		let initially_allowed = root.join("old");
		let newly_allowed = root.join("new");
		create_dir(&initially_allowed);
		create_dir(&newly_allowed);

		let initial_filter = Arc::new(DirFilter::denying([newly_allowed.clone()]));
		let mut harness = FsHarness::start(
			case,
			vec![WatchedPath::recursive(&root)],
			true,
			initial_filter,
		)
		.await;

		let initially_ignored = newly_allowed.join("before.txt");
		let initially_seen = initially_allowed.join("before.txt");
		write_file(&initially_ignored, "ignored before replacement");
		write_file(&initially_seen, "seen before replacement");
		harness
			.wait_for_any_path(&[initially_seen], "checking the initial filterer")
			.await;
		harness
			.assert_no_path_under(
				std::slice::from_ref(&newly_allowed),
				"checking the initially denied subtree",
			)
			.await;

		let replacement = Arc::new(DirFilter::denying([initially_allowed.clone()]));
		harness.set_filterer(replacement).await;

		#[cfg(target_os = "linux")]
		if case.watcher == Watcher::Native {
			assert_linux_inotify_registration(&root, &initially_allowed, false);
			assert_linux_inotify_registration(&root, &newly_allowed, true);
		}

		let newly_ignored = initially_allowed.join("after.txt");
		let newly_seen = newly_allowed.join("after.txt");
		write_file(&newly_ignored, "ignored after replacement");
		write_file(&newly_seen, "seen after replacement");
		harness
			.wait_for_any_path(&[newly_seen], "checking the replacement filterer")
			.await;
		harness
			.assert_no_path_under(&[initially_allowed], "checking the live-pruned subtree")
			.await;
		harness.shutdown().await;
	}
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn removing_overlapping_parent_root_keeps_explicit_child_root_active() {
	for case in managed_watcher_cases() {
		let temp = make_tempdir(case, "overlap");
		let outer = temp.path().join("outer");
		let child = outer.join("shared-child");
		let sibling = outer.join("parent-only");
		create_dir(&child);
		create_dir(&sibling);

		let mut harness = FsHarness::start(
			case,
			vec![
				WatchedPath::recursive(&outer),
				WatchedPath::recursive(&child),
			],
			true,
			(),
		)
		.await;
		harness
			.set_paths(vec![WatchedPath::recursive(&child)])
			.await;

		let outside_remaining_root = sibling.join("ignored.txt");
		let inside_remaining_root = child.join("seen.txt");
		write_file(&outside_remaining_root, "outside");
		write_file(&inside_remaining_root, "inside");
		harness
			.wait_for_any_path(&[inside_remaining_root], "checking the retained child root")
			.await;
		harness
			.assert_no_path_under(&[sibling], "checking the removed parent root")
			.await;
		harness.shutdown().await;
	}
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn configured_root_is_reacquired_after_delete_and_recreate() {
	for case in managed_watcher_cases() {
		let temp = make_tempdir(case, "recreate-root");
		let root = temp.path().join("configured-root");
		create_dir(&root.join("old"));
		let mut harness =
			FsHarness::start(case, vec![WatchedPath::recursive(&root)], true, ()).await;

		std::fs::remove_dir_all(&root)
			.unwrap_or_else(|error| panic!("failed to remove configured root {root:?}: {error}"));
		harness
			.wait_for_any_path_matching(
				std::slice::from_ref(&root),
				event_is_remove,
				"observing configured root deletion",
			)
			.await;
		harness.drain_events();

		let recreated_deep = root.join("new/deep");
		let seed = recreated_deep.join("seed.txt");
		create_dir(&recreated_deep);
		write_file(&seed, "already populated");
		harness
			.wait_for_any_path_matching(
				&[root.clone(), recreated_deep.clone(), seed],
				event_is_create,
				"observing configured root recreation",
			)
			.await;

		let probe = recreated_deep.join("probe.txt");
		write_file(&probe, "probe");
		harness
			.wait_for_any_path(&[probe], "checking the reacquired configured root")
			.await;
		harness.shutdown().await;
	}
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "windows"))]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_file_root_survives_repeated_replacement() {
	let case = WatcherCase {
		name: "native",
		watcher: Watcher::Native,
	};
	let temp = make_tempdir(case, "replace-file-root");
	let file = temp.path().join("watched.txt");
	write_file(&file, "initial");
	let mut harness = FsHarness::start(case, vec![WatchedPath::recursive(&file)], true, ()).await;
	harness.drain_until_quiet().await;

	for generation in 1..=2 {
		let replacement = temp.path().join(format!("replacement-{generation}.txt"));
		replace_file(&file, &replacement, &format!("replacement {generation}"));
		harness
			.wait_for_any_path(
				std::slice::from_ref(&file),
				&format!("observing atomic replacement {generation}"),
			)
			.await;
		harness.drain_until_quiet().await;

		write_file(&file, &format!("modified replacement {generation}"));
		harness
			.wait_for_any_path_matching(
				std::slice::from_ref(&file),
				event_is_non_name_modify,
				&format!("checking replacement {generation} remains watched"),
			)
			.await;
		harness.drain_until_quiet().await;
	}

	harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_nonrecursive_root_does_not_report_grandchildren() {
	for case in managed_watcher_cases() {
		let temp = make_tempdir(case, "nonrecursive");
		let root = temp.path().join("root");
		let child = root.join("child");
		create_dir(&child);
		let mut harness =
			FsHarness::start(case, vec![WatchedPath::non_recursive(&root)], true, ()).await;

		let grandchild = child.join("grandchild.txt");
		let direct = root.join("direct.txt");
		write_file(&grandchild, "must not be observed");
		write_file(&direct, "must be observed");
		harness
			.wait_for_any_path(&[direct], "checking a direct child of a nonrecursive root")
			.await;
		harness
			.assert_no_path_under(
				&[grandchild],
				"checking a grandchild of a nonrecursive root",
			)
			.await;
		harness.shutdown().await;
	}
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn follow_symlinks_controls_watching_external_directory_targets() {
	use std::os::unix::fs::symlink;

	for case in managed_watcher_cases() {
		let temp = make_tempdir(case, "symlinks");
		let temp_path = std::fs::canonicalize(temp.path())
			.unwrap_or_else(|error| panic!("failed to canonicalize temporary directory: {error}"));
		let root = temp_path.join("root");
		let target = temp_path.join("target");
		let link = root.join("link");
		create_dir(&root);
		create_dir(&target);
		symlink(&target, &link)
			.unwrap_or_else(|error| panic!("failed to symlink {link:?} to {target:?}: {error}"));

		let mut no_follow =
			FsHarness::start(case, vec![WatchedPath::recursive(&root)], false, ()).await;
		let unseen_target_file = target.join("not-followed.txt");
		let direct = root.join("direct.txt");
		write_file(&unseen_target_file, "target change");
		write_file(&direct, "synchronising accepted change");
		no_follow
			.wait_for_any_path(&[direct], "checking follow_symlinks=false")
			.await;
		no_follow
			.assert_no_path_under(
				&[target.clone(), link.clone()],
				"checking that a symlink target is not followed",
			)
			.await;
		no_follow.shutdown().await;

		let mut follow =
			FsHarness::start(case, vec![WatchedPath::recursive(&root)], true, ()).await;
		let target_file = target.join("followed.txt");
		let link_file = link.join("followed.txt");
		write_file(&target_file, "target change");
		follow
			.wait_for_any_path(&[target_file, link_file], "checking follow_symlinks=true")
			.await;
		follow.shutdown().await;
	}
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn path_local_scan_error_is_reported_without_stopping_worker() {
	use std::os::unix::fs::symlink;

	for case in managed_watcher_cases() {
		let temp = make_tempdir(case, "runtime-error");
		let root = temp.path().join("root");
		let accepted = root.join("accepted");
		let loop_link = root.join("loop");
		create_dir(&accepted);
		symlink("loop", &loop_link)
			.unwrap_or_else(|error| panic!("failed to create symlink loop {loop_link:?}: {error}"));

		let mut harness =
			FsHarness::start(case, vec![WatchedPath::recursive(&root)], true, ()).await;
		let mut observed_errors = Vec::new();
		let error = timeout(TEST_TIMEOUT, async {
			loop {
				match harness.errors.recv().await {
					Some(error) => {
						let is_expected = matches!(
							&error,
							RuntimeError::FsWatcher {
								kind,
								err: FsWatcherError::PathScan { path, .. },
							} if *kind == case.watcher && path == &loop_link
						);
						if is_expected {
							break error;
						}
						observed_errors.push(format!("{error:?}"));
					}
					None => panic!("{} runtime-error channel closed", case.name),
				}
			}
		})
		.await
		.unwrap_or_else(|_| {
			panic!(
				"{} watcher did not deliver the path-local scan error for {loop_link:?}; observed={observed_errors:?}; worker_finished={}",
				case.name,
				harness.task_finished()
			)
		});
		assert!(
			matches!(error, RuntimeError::FsWatcher { .. }),
			"expected a filesystem runtime error, got {error:?}"
		);

		let accepted_file = accepted.join("still-alive.txt");
		write_file(&accepted_file, "worker remains active");
		harness
			.wait_for_any_path(
				&[accepted_file],
				"checking progress after a path-local error",
			)
			.await;
		harness.shutdown().await;
	}
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FsIdentity {
	device: u64,
	inode: u64,
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct InotifyRegistration {
	fd: String,
	identity: FsIdentity,
	line: String,
}

#[cfg(target_os = "linux")]
fn assert_linux_inotify_registration(root: &Path, directory: &Path, expected: bool) {
	use std::os::unix::fs::MetadataExt;

	assert!(
		directory.starts_with(root),
		"physical source-filter proof requires {} to be inside recursive root {}",
		directory.display(),
		root.display()
	);
	let metadata = std::fs::metadata(directory)
		.unwrap_or_else(|error| panic!("failed to stat {}: {error}", directory.display()));
	assert!(
		metadata.is_dir(),
		"expected {} to be a directory",
		directory.display()
	);
	let identity = FsIdentity {
		device: linux_fdinfo_device(metadata.dev()),
		inode: metadata.ino(),
	};
	let registrations = linux_inotify_registrations();
	let found = registrations
		.iter()
		.any(|registration| registration.identity == identity);
	assert_eq!(
		found,
		expected,
		"unexpected inotify registration state for {} ({identity:?}); all process registrations:\n{}",
		directory.display(),
		RegistrationList(&registrations)
	);
}

#[cfg(target_os = "linux")]
const fn linux_fdinfo_device(device: u64) -> u64 {
	// Linux fdinfo prints the kernel's internal dev_t layout, while MetadataExt
	// exposes the userspace layout. Decode with libc, then rebuild the kernel value.
	let major = libc::major(device) as u64;
	let minor = libc::minor(device) as u64;
	(major << 20) | minor
}

#[cfg(target_os = "linux")]
fn linux_inotify_registrations() -> Vec<InotifyRegistration> {
	let entries = std::fs::read_dir("/proc/self/fdinfo")
		.unwrap_or_else(|error| panic!("failed to read /proc/self/fdinfo: {error}"));
	let mut registrations = Vec::new();

	for entry in entries {
		let entry = entry.unwrap_or_else(|error| panic!("failed to read fdinfo entry: {error}"));
		let contents = match std::fs::read_to_string(entry.path()) {
			Ok(contents) => contents,
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
			Err(error) => panic!("failed to read {}: {error}", entry.path().display()),
		};
		for line in contents.lines().filter(|line| line.starts_with("inotify ")) {
			let mut inode = None;
			let mut device = None;
			for field in line.split_ascii_whitespace() {
				if let Some(value) = field.strip_prefix("ino:") {
					inode = u64::from_str_radix(value, 16).ok();
				} else if let Some(value) = field.strip_prefix("sdev:") {
					device = u64::from_str_radix(value, 16).ok();
				}
			}
			if let (Some(device), Some(inode)) = (device, inode) {
				registrations.push(InotifyRegistration {
					fd: entry.file_name().to_string_lossy().into_owned(),
					identity: FsIdentity { device, inode },
					line: line.to_owned(),
				});
			}
		}
	}

	registrations
}

#[cfg(target_os = "linux")]
struct RegistrationList<'a>(&'a [InotifyRegistration]);

#[cfg(target_os = "linux")]
impl fmt::Display for RegistrationList<'_> {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		for registration in self.0 {
			writeln!(
				formatter,
				"fd={} {:?}: {}",
				registration.fd, registration.identity, registration.line
			)?;
		}
		Ok(())
	}
}
