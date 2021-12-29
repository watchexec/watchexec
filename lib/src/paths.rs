//! Utilities for paths and sets of paths.

use std::{
	collections::HashMap,
	ffi::OsString,
	path::{Path, PathBuf},
};

use crate::event::{Event, FileType, Tag};

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
/// - `CREATED` -> `Create(_)`
/// - `META_CHANGED` -> `Modify(Metadata(_))`
/// - `REMOVED` -> `Remove(_)`
/// - `RENAMED` -> `Modify(Name(_))`
/// - `WRITTEN` -> `Modify(Data(_))`, `Access(Close(Write))`
/// - `OTHERWISE_CHANGED` -> anything else
/// - plus `COMMON_PATH` with the common prefix of all paths (even if there's only one path).
///
/// It ignores non-path events and pathed events without event kind. Multiple events are sorted in
/// byte order and joined with the platform-specific path separator (`:` for unix, `;` for Windows).
pub fn summarise_events_to_env<'events>(
	events: impl IntoIterator<Item = &'events Event>,
) -> HashMap<&'static str, OsString> {
	#[cfg(unix)]
	const ENV_SEP: &str = ":";
	#[cfg(not(unix))]
	const ENV_SEP: &str = ";";

	let mut all_trunks = Vec::new();
	let mut kind_buckets = HashMap::new();
	for event in events {
		let (paths, trunks): (Vec<_>, Vec<_>) = event
			.paths()
			.map(|(p, ft)| {
				(
					p.to_owned(),
					match ft {
						Some(FileType::Dir) => None,
						_ => p.parent(),
					}
					.unwrap_or(p)
					.to_owned(),
				)
			})
			.unzip();
		tracing::trace!(?paths, ?trunks, "event paths");

		if paths.is_empty() {
			continue;
		}

		all_trunks.extend(trunks.clone());

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

	let common_path = common_prefix(all_trunks);

	let mut grouped_buckets = HashMap::new();
	for (kind, paths) in kind_buckets {
		use notify::event::{AccessKind::*, AccessMode::*, EventKind::*, ModifyKind::*};
		grouped_buckets
			.entry(match kind {
				Modify(Data(_)) | Access(Close(Write)) => "WRITTEN",
				Modify(Metadata(_)) => "META_CHANGED",
				Remove(_) => "REMOVED",
				Create(_) => "CREATED",
				Modify(Name(_)) => "RENAMED",
				_ => "OTHERWISE_CHANGED",
			})
			.or_insert_with(Vec::new)
			.extend(paths.into_iter().map(|p| {
				if let Some(suffix) = common_path
					.as_ref()
					.and_then(|prefix| p.strip_prefix(prefix).ok())
				{
					suffix.as_os_str().to_owned()
				} else {
					p.into_os_string()
				}
			}));
	}

	let mut res: HashMap<&'static str, OsString> = grouped_buckets
		.into_iter()
		.map(|(kind, mut paths)| {
			let mut joined =
				OsString::with_capacity(paths.iter().map(|p| p.len()).sum::<usize>() + paths.len());

			paths.sort();
			paths.into_iter().enumerate().for_each(|(i, path)| {
				if i > 0 {
					joined.push(ENV_SEP);
				}
				joined.push(path);
			});

			(kind, joined)
		})
		.collect();

	if let Some(common_path) = common_path {
		res.insert("COMMON_PATH", common_path.into_os_string());
	}

	res
}
