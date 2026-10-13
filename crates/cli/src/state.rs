use std::{
	env::var_os,
	io::Write,
	path::PathBuf,
	process::ExitCode,
	sync::{Arc, Mutex, OnceLock},
};

use watchexec::Watchexec;
use watchexec_events::{
	filekind::{CreateKind, FileEventKind},
	Event, Source, Tag,
};

use miette::{IntoDiagnostic, Result};
use tempfile::NamedTempFile;

use crate::{
	args::Args,
	socket::{SocketSet, Sockets},
};

pub type State = Arc<InnerState>;

pub async fn new(args: &Args) -> Result<State> {
	let socket_set = if args.command.socket.is_empty() {
		None
	} else {
		let mut sockets = SocketSet::create(&args.command.socket).await?;
		sockets.serve();
		Some(sockets)
	};

	Ok(Arc::new(InnerState {
		emit_file: RotatingTempFile::default(),
		socket_set,
		exit_code: Mutex::new(ExitCode::SUCCESS),
		watchexec: OnceLock::new(),
		initial_paths: Mutex::new(None),
	}))
}

#[derive(Debug)]
pub struct InnerState {
	pub emit_file: RotatingTempFile,
	pub socket_set: Option<SocketSet>,
	pub exit_code: Mutex<ExitCode>,
	/// Reference to the Watchexec instance, set after creation.
	/// Used to send synthetic events (e.g., to trigger immediate quit on error).
	pub watchexec: OnceLock<Arc<Watchexec>>,
	pub initial_paths: Mutex<Option<Vec<PathBuf>>>,
}

impl InnerState {
	pub fn events_for_emit(&self, events: Arc<[Event]>) -> (Arc<[Event]>, usize) {
		let Some(paths) = self.initial_paths.lock().unwrap().take() else {
			return (events, 0);
		};
		if paths.is_empty() {
			return (events, 0);
		}

		let initial_count = paths.len();
		let mut emitted = Vec::with_capacity(initial_count + events.len());
		emitted.extend(paths.into_iter().map(|path| Event {
			tags: vec![
				Tag::Source(Source::Filesystem),
				Tag::FileEventKind(FileEventKind::Create(CreateKind::Any)),
				Tag::Path {
					path,
					file_type: None,
				},
			],
			metadata: Default::default(),
		}));
		emitted.extend(events.iter().cloned());
		(emitted.into(), initial_count)
	}
}

#[cfg(test)]
mod initial_event_tests {
	use super::*;

	fn state(paths: Option<Vec<PathBuf>>) -> InnerState {
		InnerState {
			emit_file: RotatingTempFile::default(),
			socket_set: None,
			exit_code: Mutex::new(ExitCode::SUCCESS),
			watchexec: OnceLock::new(),
			initial_paths: Mutex::new(paths),
		}
	}

	#[test]
	fn initial_events_leave_action_events_unchanged_and_emit_once() {
		let path = PathBuf::from("/watched/existing");
		let state = state(Some(vec![path.clone()]));
		let action_events: Arc<[Event]> = vec![Event::default()].into();
		let (emitted, initial_count) = state.events_for_emit(action_events.clone());
		assert_eq!(initial_count, 1);
		assert_eq!(action_events.as_ref(), &[Event::default()]);
		assert_eq!(emitted.len(), 2);
		assert_eq!(emitted[0].paths().next().unwrap().0, path.as_path());
		assert!(emitted[0]
			.tags
			.contains(&Tag::FileEventKind(FileEventKind::Create(CreateKind::Any))));
		let (again, initial_count) = state.events_for_emit(action_events.clone());
		assert_eq!(initial_count, 0);
		assert!(Arc::ptr_eq(&again, &action_events));
	}

	#[test]
	fn disabled_and_empty_initial_events_reuse_the_action_batch() {
		for paths in [None, Some(Vec::new())] {
			let state = state(paths);
			let action_events: Arc<[Event]> = vec![Event::default()].into();
			let (emitted, initial_count) = state.events_for_emit(action_events.clone());
			assert_eq!(initial_count, 0);
			assert!(Arc::ptr_eq(&emitted, &action_events));
		}
	}
}

#[derive(Debug, Default)]
pub struct RotatingTempFile(Mutex<Option<NamedTempFile>>);

impl RotatingTempFile {
	pub fn rotate(&self) -> Result<()> {
		// implicitly drops the old file
		*self.0.lock().unwrap() = Some(
			if let Some(dir) = var_os("WATCHEXEC_TMPDIR") {
				NamedTempFile::new_in(dir)
			} else {
				NamedTempFile::new()
			}
			.into_diagnostic()?,
		);
		Ok(())
	}

	pub fn write(&self, data: &[u8]) -> Result<()> {
		if let Some(file) = self.0.lock().unwrap().as_mut() {
			file.write_all(data).into_diagnostic()?;
		}

		Ok(())
	}

	pub fn path(&self) -> PathBuf {
		if let Some(file) = self.0.lock().unwrap().as_ref() {
			file.path().to_owned()
		} else {
			PathBuf::new()
		}
	}
}
