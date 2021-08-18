use std::{
	collections::{HashMap, HashSet},
	path::PathBuf,
};

use tokio::sync::{mpsc, watch};
use tracing::{debug, trace};

use crate::{
	error::{CriticalError, RuntimeError},
	event::{Event, Particle, Source},
};

/// What kind of filesystem watcher to use.
///
/// For now only native and poll watchers are supported. In the future there may be additional
/// watchers available on some platforms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Watcher {
	Native,
	Poll,
}

impl Default for Watcher {
	fn default() -> Self {
		Self::Native
	}
}

impl Watcher {
	fn create(
		self,
		f: impl notify::EventFn,
	) -> Result<Box<dyn notify::Watcher + Send>, RuntimeError> {
		match self {
			Self::Native => notify::RecommendedWatcher::new(f).map(|w| Box::new(w) as _),
			Self::Poll => notify::PollWatcher::new(f).map(|w| Box::new(w) as _),
		}
		.map_err(|err| RuntimeError::FsWatcherCreate { kind: self, err })
	}
}

/// The working data set of the filesystem worker.
///
/// This is marked non-exhaustive so new configuration can be added without breaking.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct WorkingData {
	pub pathset: Vec<PathBuf>,
	pub watcher: Watcher,
}

/// Launch the filesystem event worker.
///
/// While you can run several, you should only have one.
///
/// This only does a bare minimum of setup; to actually start the work, you need to set a non-empty pathset on the
/// [`WorkingData`] with the [`watch`] channel, and send a notification. Take care _not_ to drop the watch sender:
/// this will cause the worker to stop gracefully, which may not be what was expected.
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
					watcher.insert(w);
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
				if let Err(err) = w.unwatch(&path) {
					errors
						.send(RuntimeError::FsWatcherPathRemove {
							path,
							kind: watcher_type,
							err,
						})
						.await?;
				} else {
					pathset.remove(&path);
				}
			}

			for path in to_watch {
				trace!(?path, "adding path to the watcher");
				if let Err(err) = w.watch(&path, notify::RecursiveMode::Recursive) {
					errors
						.send(RuntimeError::FsWatcherPathAdd {
							path,
							kind: watcher_type,
							err,
						})
						.await?;
				} else {
					pathset.insert(path);
				}
			}
		}
	}

	debug!("ending file watcher");
	Ok(())
}

fn process_event(
	nev: Result<notify::Event, notify::Error>,
	kind: Watcher,
	n_events: mpsc::Sender<Event>,
) -> Result<(), RuntimeError> {
	let nev = nev.map_err(|err| RuntimeError::FsWatcherEvent { kind, err })?;

	let mut particulars = Vec::with_capacity(4);
	particulars.push(Particle::Source(Source::Filesystem));

	for path in nev.paths {
		particulars.push(Particle::Path(dunce::canonicalize(path)?));
	}

	if let Some(pid) = nev.attrs.process_id() {
		particulars.push(Particle::Process(pid));
	}

	let ev = Event {
		particulars,
		metadata: HashMap::new(), // TODO
	};

	trace!(event = ?ev, "processed notify event into watchexec event");
	n_events
		.try_send(ev)
		.map_err(|err| RuntimeError::EventChannelTrySend {
			ctx: "fs watcher",
			err,
		})?;

	Ok(())
}
