//! Event source for changes to files and directories.

use std::{
	collections::{HashMap, HashSet},
	fs::metadata,
	mem::take,
	path::{Path, PathBuf},
	sync::Arc,
	time::Duration,
};

use async_priority_channel as priority;
use normalize_path::NormalizePath;
use tokio::sync::mpsc;
use tracing::{debug, error, trace};
use watchexec_events::{Event, Priority, Source, Tag};

use crate::{
	error::{CriticalError, FsWatcherError, RuntimeError},
	Config,
};

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
	#[default]
	Native,

	/// Notify’s [poll watcher][notify::PollWatcher] with a custom interval.
	Poll(Duration),
}

impl Watcher {
	fn create(
		self,
		f: impl notify::EventHandler,
	) -> Result<Box<dyn notify::Watcher + Send>, CriticalError> {
		use notify::{Config, Watcher as _};

		match self {
			Self::Native => {
				notify::RecommendedWatcher::new(f, Config::default()).map(|w| Box::new(w) as _)
			}
			Self::Poll(delay) => {
				notify::PollWatcher::new(f, Config::default().with_poll_interval(delay))
					.map(|w| Box::new(w) as _)
			}
		}
		.map_err(|err| CriticalError::FsWatcherInit {
			kind: self,
			err: if cfg!(target_os = "linux")
				&& (matches!(err.kind, notify::ErrorKind::MaxFilesWatch)
					|| matches!(err.kind, notify::ErrorKind::Io(ref ioerr) if ioerr.raw_os_error() == Some(28)))
			{
				FsWatcherError::TooManyWatches(err)
			} else if cfg!(target_os = "linux")
				&& matches!(err.kind, notify::ErrorKind::Io(ref ioerr) if ioerr.raw_os_error() == Some(24))
			{
				FsWatcherError::TooManyHandles(err)
			} else {
				FsWatcherError::Create(err)
			},
		})
	}
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

	let mut watcher_type = Watcher::default();
	let mut watcher = None;
	let mut pathset = HashSet::new();

	let mut config_watch = config.watch();
	loop {
		config_watch.next().await;
		trace!("filesystem worker got a config change");

		if config.pathset.get().is_empty() {
			trace!(
				"{}",
				if pathset.is_empty() {
					"no watched paths, no watcher needed"
				} else {
					"no more watched paths, dropping watcher"
				}
			);
			watcher.take();
			pathset.clear();
			continue;
		}

		// now we know the watcher should be alive, so let's start it if it's not already:

		let config_watcher = config.file_watcher.get();
		if watcher.is_none() || watcher_type != config_watcher {
			debug!(kind=?config_watcher, "creating new watcher");
			let n_errors = errors.clone();
			let n_events = events.clone();
			watcher_type = config_watcher;
			watcher = config_watcher
				.create(move |nev: Result<notify::Event, notify::Error>| {
					trace!(event = ?nev, "receiving possible event from watcher");
					if let Err(e) = process_event(nev, config_watcher, &n_events) {
						n_errors.try_send(e).ok();
					}
				})
				.map(Some)?;
		}

		// now let's calculate which paths we should add to the watch, and which we should drop:

		let config_pathset = config.pathset.get();
		let (to_watch, to_drop) = if pathset.is_empty() {
			// if the current pathset is empty, we can take a shortcut
			(config_pathset, Vec::new())
		} else {
			let mut to_watch = Vec::with_capacity(config_pathset.len());
			let mut to_drop = Vec::with_capacity(pathset.len());

			for path in &pathset {
				if !config_pathset.contains(path) {
					to_drop.push(path.clone()); // try dropping the clone?
				}
			}

			for path in config_pathset {
				if !pathset.contains(&path) {
					to_watch.push(path);
				}
			}

			(to_watch, to_drop)
		};

		// now apply it to the watcher

		let Some(watcher) = watcher.as_mut() else {
			panic!("BUG: watcher should exist at this point");
		};

		debug!(?to_watch, ?to_drop, "applying changes to the watcher");

		for path in to_drop {
			trace!(?path, "removing path from the watcher");
			if let Err(err) = watcher.unwatch(path.as_ref()) {
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
			if let Err(err) = watcher.watch(path.as_ref(), notify::RecursiveMode::Recursive) {
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

		errs.push(RuntimeError::FsWatcher {
			kind,
			err: if rm {
				FsWatcherError::PathRemove { path, err: e }
			} else {
				FsWatcherError::PathAdd { path, err: e }
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

	let mut tags = Vec::with_capacity(4);
	tags.push(Tag::Source(Source::Filesystem));
	tags.push(Tag::FileEventKind(nev.kind));

	for path in nev.paths {
		// possibly pull file_type from whatever notify (or the native driver) returns?
		tags.push(Tag::Path {
			file_type: metadata(&path).ok().map(|m| m.file_type().into()),
			path: path.normalize(),
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
		.try_send(ev, Priority::Normal)
		.map_err(|err| RuntimeError::EventChannelTrySend {
			ctx: "fs watcher",
			err,
		})?;

	Ok(())
}
