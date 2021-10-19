//! Event source for changes to files and directories.

use std::{
	collections::{HashMap, HashSet},
	fs::metadata,
	mem::take,
	path::{Path, PathBuf},
	sync::{Arc, Mutex},
	time::Duration,
};

use notify::Watcher as _;
use tokio::sync::{mpsc, watch};
use tracing::{debug, error, trace};

use crate::{
	error::{CriticalError, RuntimeError},
	event::{Event, Source, Tag},
};

/// What kind of filesystem watcher to use.
///
/// For now only native and poll watchers are supported. In the future there may be additional
/// watchers available on some platforms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Watcher {
	/// The Notify-recommended watcher on the platform.
	///
	/// For platforms Notify supports, that's a [native implementation][notify::RecommendedWatcher],
	/// for others it's polling with a default interval.
	Native,

	/// Notify’s [poll watcher][notify::PollWatcher] with a custom interval.
	Poll(Duration),
}

impl Default for Watcher {
	fn default() -> Self {
		Self::Native
	}
}

impl Watcher {
	fn create(
		self,
		f: impl notify::EventHandler,
	) -> Result<Box<dyn notify::Watcher + Send>, RuntimeError> {
		match self {
			Self::Native => notify::RecommendedWatcher::new(f).map(|w| Box::new(w) as _),
			Self::Poll(delay) => notify::PollWatcher::with_delay(Arc::new(Mutex::new(f)), delay)
				.map(|w| Box::new(w) as _),
		}
		.map_err(|err| RuntimeError::FsWatcherCreate { kind: self, err })
	}
}

/// The configuration of the [fs][self] worker.
///
/// This is marked non-exhaustive so new configuration can be added without breaking.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct WorkingData {
	/// The set of paths to be watched.
	pub pathset: Vec<WatchedPath>,

	/// The kind of watcher to be used.
	pub watcher: Watcher,
}

/// A path to watch.
///
/// This is currently only a wrapper around a [`PathBuf`], but may be augmented in the future.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WatchedPath(PathBuf);

impl From<PathBuf> for WatchedPath {
	fn from(path: PathBuf) -> Self {
		Self(path)
	}
}

impl From<&str> for WatchedPath {
	fn from(path: &str) -> Self {
		Self(path.into())
	}
}

impl From<&Path> for WatchedPath {
	fn from(path: &Path) -> Self {
		Self(path.into())
	}
}

impl From<WatchedPath> for PathBuf {
	fn from(path: WatchedPath) -> Self {
		path.0
	}
}

impl AsRef<Path> for WatchedPath {
	fn as_ref(&self) -> &Path {
		self.0.as_ref()
	}
}

/// Launch the filesystem event worker.
///
/// While you can run several, you should only have one.
///
/// This only does a bare minimum of setup; to actually start the work, you need to set a non-empty
/// pathset on the [`WorkingData`] with the [`watch`] channel, and send a notification. Take care
/// _not_ to drop the watch sender: this will cause the worker to stop gracefully, which may not be
/// what was expected.
///
/// Note that the paths emitted by the watcher are canonicalised. No guarantee is made about the
/// implementation or output of that canonicalisation (i.e. it might not be `std`'s).
///
/// # Examples
///
/// Direct usage:
///
/// ```no_run
/// use tokio::sync::{mpsc, watch};
/// use watchexec::fs::{worker, WorkingData};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let (ev_s, _) = mpsc::channel(1024);
///     let (er_s, _) = mpsc::channel(64);
///     let (wd_s, wd_r) = watch::channel(WorkingData::default());
///
///     let mut wkd = WorkingData::default();
///     wkd.pathset = vec![".".into()];
///     wd_s.send(wkd)?;
///
///     worker(wd_r, er_s, ev_s).await?;
///     Ok(())
/// }
/// ```
pub async fn worker(
	mut working: watch::Receiver<WorkingData>,
	errors: mpsc::Sender<RuntimeError>,
	events: mpsc::Sender<Event>,
) -> Result<(), CriticalError> {
	debug!("launching filesystem worker");

	let mut watcher_type = Watcher::default();
	let mut watcher = None;
	let mut pathset = HashSet::new();

	while working.changed().await.is_ok() {
		// In separate scope so we drop the working read lock as early as we can
		let (new_watcher, to_watch, to_drop) = {
			let data = working.borrow();
			trace!(?data, "filesystem worker got a working data change");

			if data.pathset.is_empty() {
				trace!("no more watched paths, dropping watcher");
				watcher.take();
				pathset.drain();
				continue;
			}

			if watcher.is_none() || watcher_type != data.watcher {
				pathset.drain();

				(Some(data.watcher), data.pathset.clone(), Vec::new())
			} else {
				let mut to_watch = Vec::with_capacity(data.pathset.len());
				let mut to_drop = Vec::with_capacity(pathset.len());
				for path in data.pathset.iter() {
					if !pathset.contains(path) {
						to_watch.push(path.clone());
					}
				}

				for path in pathset.iter() {
					if !data.pathset.contains(path) {
						to_drop.push(path.clone());
					}
				}

				(None, to_watch, to_drop)
			}
		};

		if let Some(kind) = new_watcher {
			debug!(?kind, "creating new watcher");
			let n_errors = errors.clone();
			let n_events = events.clone();
			match kind.create(move |nev: Result<notify::Event, notify::Error>| {
				trace!(event = ?nev, "receiving possible event from watcher");
				if let Err(e) = process_event(nev, kind, n_events.clone()) {
					n_errors.try_send(e).ok();
				}
			}) {
				Ok(w) => {
					watcher = Some(w);
					watcher_type = kind;
				}
				Err(e) => {
					errors.send(e).await?;
				}
			}
		}

		if let Some(w) = watcher.as_mut() {
			debug!(?to_watch, ?to_drop, "applying changes to the watcher");

			for path in to_drop {
				trace!(?path, "removing path from the watcher");
				if let Err(err) = w.unwatch(path.as_ref()) {
					error!(?err, "notify unwatch() error");
					for e in notify_multi_path_errors(watcher_type, path, err, true) {
						errors.send(e).await?;
					}
				} else {
					pathset.remove(&path);
				}
			}

			for path in to_watch {
				trace!(?path, "adding path to the watcher");
				if let Err(err) = w.watch(path.as_ref(), notify::RecursiveMode::Recursive) {
					error!(?err, "notify watch() error");
					for e in notify_multi_path_errors(watcher_type, path, err, false) {
						errors.send(e).await?;
					}
				// TODO: unwatch and re-watch manually while ignoring all the erroring paths
				// See https://github.com/watchexec/watchexec/issues/218
				} else {
					pathset.insert(path);
				}
			}
		}
	}

	debug!("ending file watcher");
	Ok(())
}

fn notify_multi_path_errors(
	kind: Watcher,
	path: WatchedPath,
	mut err: notify::Error,
	rm: bool,
) -> Vec<RuntimeError> {
	let mut paths = take(&mut err.paths);
	if paths.is_empty() {
		paths.push(path.into());
	}

	let generic = err.to_string();
	let mut err = Some(err);

	let mut errs = Vec::with_capacity(paths.len());
	for path in paths {
		let e = err
			.take()
			.unwrap_or_else(|| notify::Error::generic(&generic))
			.add_path(path.clone());

		errs.push(if rm {
			RuntimeError::FsWatcherPathRemove { path, kind, err: e }
		} else {
			RuntimeError::FsWatcherPathAdd { path, kind, err: e }
		});
	}

	errs
}

fn process_event(
	nev: Result<notify::Event, notify::Error>,
	kind: Watcher,
	n_events: mpsc::Sender<Event>,
) -> Result<(), RuntimeError> {
	let nev = nev.map_err(|err| RuntimeError::FsWatcherEvent { kind, err })?;

	let mut tags = Vec::with_capacity(4);
	tags.push(Tag::Source(Source::Filesystem));
	tags.push(Tag::FileEventKind(nev.kind));

	for path in nev.paths {
		// possibly pull file_type from whatever notify (or the native driver) returns?
		tags.push(Tag::Path {
			file_type: metadata(&path).ok().map(|m| m.file_type().into()),
			path: dunce::canonicalize(path)?,
		});
	}

	if let Some(pid) = nev.attrs.process_id() {
		tags.push(Tag::Process(pid));
	}

	let mut metadata = HashMap::new();

	if let Some(uid) = nev.attrs.info() {
		metadata.insert("file-event-info".to_string(), vec![uid.to_string()]);
	}

	if let Some(src) = nev.attrs.source() {
		metadata.insert("notify-backend".to_string(), vec![src.to_string()]);
	}

	let ev = Event { tags, metadata };

	trace!(event = ?ev, "processed notify event into watchexec event");
	n_events
		.try_send(ev)
		.map_err(|err| RuntimeError::EventChannelTrySend {
			ctx: "fs watcher",
			err,
		})?;

	Ok(())
}
