use std::{collections::{HashMap, HashSet}, path::PathBuf};

use tokio::{sync::{mpsc, watch}};
use tracing::{debug, trace};

use crate::{error::{CriticalError, RuntimeError}, event::{Event, Particle, Source}};

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
	fn create(self, f: impl notify::EventFn) -> Result<Box<dyn notify::Watcher>, RuntimeError> {
		match self {
			Self::Native => notify::RecommendedWatcher::new(f).map(|w| Box::new(w) as Box<dyn notify::Watcher>),
			Self::Poll => notify::PollWatcher::new(f).map(|w| Box::new(w) as Box<dyn notify::Watcher>),
		}.map_err(|err| RuntimeError::FsWatcherCreate { kind: self, err })
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

/// Launch a filesystem event worker.
///
/// This only does a bare minimum of setup; to actually start the work, you need to set a non-empty pathset on the
/// [`WorkingData`] with the [`watch`] channel.
pub async fn worker(
	mut working: watch::Receiver<WorkingData>,
	errors: mpsc::Sender<RuntimeError>,
	events: mpsc::Sender<Event>,
) -> Result<(), CriticalError> {
	debug!("launching filesystem worker");

	let mut watcher_type = Watcher::default();
	let mut watcher: Option<Box<dyn notify::Watcher>> = None;
	let mut pathset: HashSet<PathBuf> = HashSet::new();

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
			match kind.create(move |nev: Result<notify::Event, notify::Error> | {
				trace!(event = ?nev, "receiving possible event from watcher");

				match nev {
					Err(err) => {
						n_errors.try_send(RuntimeError::FsWatcherEvent { kind, err }).ok();
					},

					Ok(nev) => {
						let mut particulars = Vec::with_capacity(4);
						particulars.push(Particle::Source(Source::Filesystem));

						for path in nev.paths {
							particulars.push(Particle::Path(path));
						}

						if let Some(pid) = nev.attrs.process_id() {
							particulars.push(Particle::Process(pid));
						}

						let ev = Event {
							particulars,
							metadata: HashMap::new(), // TODO
						};

						trace!(event = ?ev, "processed notify event into watchexec event");
						if let Err(err) = n_events.try_send(ev) {
							n_errors.try_send(RuntimeError::EventChannelSend {
								ctx: "fs watcher",
								err,
							}).ok();
						}
					}
				}
			}) {
				Ok(w) => {
					watcher.insert(w);
					watcher_type = kind;
				},
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
					errors.send(RuntimeError::FsWatcherPathRemove { path, kind: watcher_type, err }).await?;
				} else {
					pathset.remove(&path);
				}
			}

			for path in to_watch {
				trace!(?path, "adding path to the watcher");
				if let Err(err) = w.watch(&path, notify::RecursiveMode::Recursive) {
					errors.send(RuntimeError::FsWatcherPathAdd { path, kind: watcher_type, err }).await?;
				} else {
					pathset.insert(path);
				}
			}
		}
	}

	Ok(())
}
