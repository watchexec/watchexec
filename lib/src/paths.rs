//! Utilities for paths and sets of paths.

use std::{
	collections::HashMap,
	ffi::{OsStr, OsString},
	path::{Path, PathBuf},
};

use crate::event::{Event, Tag};

/// Returns the longest common prefix of all given paths.
///
/// This is a utility function which is useful for finding the common root of a set of origins.
///
/// Returns `None` if zero paths are given or paths share no common prefix.
pub fn common_prefix<I, P>(paths: I) -> Option<PathBuf>
where
	I: IntoIterator<Item = P>,
	P: AsRef<Path>,
{
	let mut paths = paths.into_iter();
	let first_path = paths.next().map(|p| p.as_ref().to_owned());
	let mut longest_path = if let Some(ref p) = first_path {
		p.components().collect::<Vec<_>>()
	} else {
		return None;
	};

	for path in paths {
		let mut greatest_distance = 0;
		for component_pair in path.as_ref().components().zip(longest_path.iter()) {
			if component_pair.0 != *component_pair.1 {
				break;
			}

			greatest_distance += 1;
		}

		if greatest_distance != longest_path.len() {
			longest_path.truncate(greatest_distance);
		}
	}

	if longest_path.is_empty() {
		None
	} else {
		let mut result = PathBuf::new();
		for component in longest_path {
			result.push(component.as_os_str());
		}
		Some(result)
	}
}

/// Summarise [`Event`]s as a set of environment variables by category.
///
/// - `WRITTEN` -> `Modify(Data(_))`, `Access(Close(Write))`
/// - `META_CHANGED` -> `Modify(Metadata(_))`
/// - `REMOVED` -> `Remove(_)`
/// - `CREATED` -> `Create(_)`
/// - `RENAMED` -> `Modify(Name(_))`
/// - `OTHERWISE_CHANGED` -> anything else
///
/// It ignores non-path events and pathed events without event kind.
pub fn summarise_events_to_env<I, E>(events: I) -> HashMap<&'static OsStr, OsString>
where
	I: IntoIterator<Item = E>,
	E: AsRef<Event>,
{
	#[cfg(unix)]
	const ENV_SEP: &str = ":";
	#[cfg(not(unix))]
	const ENV_SEP: &str = ";";

	let mut kind_buckets = HashMap::new();
	for event in events {
		let event = event.as_ref();
		let paths = event.paths().map(|p| p.to_owned()).collect::<Vec<_>>();
		if paths.is_empty() {
			continue;
		}

		// usually there's only one but just in case
		for kind in event.tags.iter().filter_map(|t| {
			if let Tag::FileEventKind(kind) = t {
				Some(kind.clone())
			} else {
				None
			}
		}) {
			kind_buckets
				.entry(kind)
				.or_insert_with(Vec::new)
				.extend(paths.clone());
		}
	}

	let mut grouped_buckets = HashMap::new();
	for (kind, paths) in kind_buckets {
		use notify::event::{AccessKind::*, AccessMode::*, EventKind::*, ModifyKind::*};
		grouped_buckets
			.entry(OsStr::new(match kind {
				Modify(Data(_)) | Access(Close(Write)) => "WRITTEN",
				Modify(Metadata(_)) => "META_CHANGED",
				Remove(_) => "REMOVED",
				Create(_) => "CREATED",
				Modify(Name(_)) => "RENAMED",
				_ => "OTHERWISE_CHANGED",
			}))
			.or_insert_with(Vec::new)
			.extend(paths.into_iter().map(|p| p.into_os_string()));
	}

	grouped_buckets
		.into_iter()
		.map(|(kind, paths)| {
			let mut joined =
				OsString::with_capacity(paths.iter().map(|p| p.len()).sum::<usize>() + paths.len());

			for (i, path) in paths.into_iter().enumerate() {
				if i > 0 {
					joined.push(ENV_SEP);
				}
				joined.push(path);
			}

			(kind, joined)
		})
		.collect()
}
