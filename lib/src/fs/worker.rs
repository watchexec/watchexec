use std::{
	collections::{HashMap, HashSet},
	fs::metadata,
};

use tokio::sync::{mpsc, watch};
use tracing::{debug, trace, warn};

use crate::{
	error::{CriticalError, RuntimeError},
	event::{Event, Source, Tag},
};

use super::{recursor::PathSet, Watcher, WorkingData};

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

	// the effective pathset
	let mut pathset = PathSet::default();

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

				(Some(data.watcher), data.pathset.clone(), HashSet::new())
			} else {
				let mut to_watch = HashSet::with_capacity(data.pathset.len());
				let mut to_drop = HashSet::with_capacity(pathset.len());
				for path in data.pathset.iter() {
					if !pathset.contains(path) {
						to_watch.insert(path.clone());
					}
				}

				for path in pathset.iter() {
					if !data.pathset.contains(path) {
						to_drop.insert(path.clone());
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
				if let Err(err) = path.unwatch(watcher_type, w, &mut pathset) {
					errors.send(err).await?;
				}
			}

			for path in to_watch {
				trace!(?path, "adding path to the watcher");
				if let Err(err) = path.watch(watcher_type, w, &mut pathset) {
					errors.send(err).await?;
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

	let mut tags = Vec::with_capacity(4);
	tags.push(Tag::Source(Source::Filesystem));
	tags.push(Tag::FileEventKind(nev.kind));

	for path in nev.paths {
		// possibly pull file_type from whatever notify (or the native driver) returns?
		tags.push(Tag::Path {
			file_type: metadata(&path).ok().map(|m| m.file_type().into()),
			path: dunce::canonicalize(&path).unwrap_or_else(|err| {
				warn!(?err, ?path, "failed to canonicalise event path");
				path
			}),
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
