//! Utilities for paths and sets of paths.

use std::path::{Path, PathBuf};

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
