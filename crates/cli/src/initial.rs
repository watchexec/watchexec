use std::{
	collections::HashSet,
	fs,
	path::{Path, PathBuf},
};

use tracing::warn;
use watchexec::WatchedPath;
use watchexec_events::{
	filekind::{CreateKind, FileEventKind},
	Event, FileType, Source, Tag,
};

/// Collect synthetic create events for the current contents of the watch roots.
pub fn collect(paths: &[WatchedPath], follow_symlinks: bool) -> Vec<Event> {
	let mut events = Vec::new();
	let mut seen_paths = HashSet::new();
	let mut visited_dirs = HashSet::new();

	for watched_path in paths {
		let path = watched_path.as_ref();
		if watched_path.is_recursive() {
			visit_recursive(
				path,
				follow_symlinks,
				&mut seen_paths,
				&mut visited_dirs,
				&mut events,
			);
		} else {
			visit_non_recursive(path, &mut seen_paths, &mut events);
		}
	}

	events
}

fn visit_recursive(
	path: &Path,
	follow_symlinks: bool,
	seen_paths: &mut HashSet<PathBuf>,
	visited_dirs: &mut HashSet<PathBuf>,
	events: &mut Vec<Event>,
) {
	let Ok(metadata) = fs::symlink_metadata(path) else {
		warn!(?path, "failed to inspect path for initial events");
		return;
	};
	add_event(path, metadata.file_type().into(), seen_paths, events);

	let is_directory = metadata.is_dir()
		|| (follow_symlinks
			&& metadata.file_type().is_symlink()
			&& fs::metadata(path).is_ok_and(|m| m.is_dir()));
	if !is_directory || !visited_dirs.insert(canonical_for_visit(path)) {
		return;
	}

	let Ok(entries) = fs::read_dir(path) else {
		warn!(?path, "failed to enumerate path for initial events");
		return;
	};
	for entry in entries {
		match entry {
			Ok(entry) => visit_recursive(
				&entry.path(),
				follow_symlinks,
				seen_paths,
				visited_dirs,
				events,
			),
			Err(error) => warn!(?path, %error, "failed to read directory entry for initial events"),
		}
	}
}

fn visit_non_recursive(path: &Path, seen_paths: &mut HashSet<PathBuf>, events: &mut Vec<Event>) {
	let Ok(metadata) = fs::symlink_metadata(path) else {
		warn!(?path, "failed to inspect path for initial events");
		return;
	};
	add_event(path, metadata.file_type().into(), seen_paths, events);
	if !metadata.is_dir() {
		return;
	}

	let Ok(entries) = fs::read_dir(path) else {
		warn!(?path, "failed to enumerate path for initial events");
		return;
	};
	for entry in entries {
		match entry {
			Ok(entry) => {
				let entry_path = entry.path();
				if let Ok(metadata) = fs::symlink_metadata(&entry_path) {
					add_event(&entry_path, metadata.file_type().into(), seen_paths, events);
				}
			}
			Err(error) => warn!(?path, %error, "failed to read directory entry for initial events"),
		}
	}
}

fn add_event(
	path: &Path,
	file_type: FileType,
	seen_paths: &mut HashSet<PathBuf>,
	events: &mut Vec<Event>,
) {
	if !seen_paths.insert(path.to_owned()) {
		return;
	}
	events.push(Event {
		tags: vec![
			Tag::Source(Source::Filesystem),
			Tag::FileEventKind(FileEventKind::Create(CreateKind::Any)),
			Tag::Path {
				path: path.to_owned(),
				file_type: Some(file_type),
			},
		],
		metadata: Default::default(),
	});
}

fn canonical_for_visit(path: &Path) -> PathBuf {
	fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn recursive_collection_includes_descendants() {
		let tempdir = tempfile::tempdir().unwrap();
		fs::create_dir(tempdir.path().join("nested")).unwrap();
		fs::write(tempdir.path().join("root.txt"), "root").unwrap();
		fs::write(tempdir.path().join("nested/child.txt"), "child").unwrap();

		let events = collect(&[WatchedPath::recursive(tempdir.path())], true);
		let paths: HashSet<_> = events
			.iter()
			.flat_map(Event::paths)
			.map(|(path, _)| path.to_owned())
			.collect();

		assert!(paths.contains(tempdir.path()));
		assert!(paths.contains(&tempdir.path().join("root.txt")));
		assert!(paths.contains(&tempdir.path().join("nested")));
		assert!(paths.contains(&tempdir.path().join("nested/child.txt")));
	}

	#[test]
	fn non_recursive_collection_excludes_descendants() {
		let tempdir = tempfile::tempdir().unwrap();
		fs::create_dir(tempdir.path().join("nested")).unwrap();
		fs::write(tempdir.path().join("nested/child.txt"), "child").unwrap();

		let events = collect(&[WatchedPath::non_recursive(tempdir.path())], true);
		let paths: HashSet<_> = events
			.iter()
			.flat_map(Event::paths)
			.map(|(path, _)| path.to_owned())
			.collect();

		assert!(paths.contains(tempdir.path()));
		assert!(paths.contains(&tempdir.path().join("nested")));
		assert!(!paths.contains(&tempdir.path().join("nested/child.txt")));
	}
}
