use std::{
	ffi::OsString,
	path::{Component, Path, PathBuf},
};
use watchexec_events::{Event, Tag};

pub fn normalise_event_to_unix(event: &Event, with_prefix: bool) -> Event {
	let mut path_normalised_event = event.clone();
	for mut tag in &mut path_normalised_event.tags {
		if let Tag::Path { ref mut path, .. } = &mut tag {
			*path = normalise_path_to_unix(path, with_prefix);
		}
	}
	path_normalised_event
}

pub fn normalise_path_to_unix(path: &Path, with_prefix: bool) -> PathBuf {
	let mut newpath = OsString::with_capacity(path.as_os_str().len());
	let mut skip_root = false;
	for component in path.components() {
		if matches!(component, Component::Prefix(_)) {
			if with_prefix {
				newpath.push(component.as_os_str());
				skip_root = true;
			}
		} else if matches!(component, Component::RootDir) && skip_root {
			// skip
		} else {
			newpath.push("/");
			newpath.push(component.as_os_str());
		}
	}

	PathBuf::from(newpath)
}

#[cfg(test)]
#[test]
fn test_normalise_path_to_unix() {
	assert_eq!(
		normalise_path_to_unix(Path::new("C:\\Users\\foo\\bar"), false),
		PathBuf::from("/Users/foo/bar")
	);
	assert_eq!(
		normalise_path_to_unix(Path::new("C:\\Users\\foo\\bar"), true),
		PathBuf::from("C:/Users/foo/bar")
	);
	assert_eq!(
		normalise_path_to_unix(Path::new("E:\\_temp\\folder_to_watch\\private"), false),
		PathBuf::from("/_temp/folder_to_watch/private")
	);
	assert_eq!(
		normalise_path_to_unix(Path::new("E:\\_temp\\folder_to_watch\\private"), true),
		PathBuf::from("E:/_temp/folder_to_watch/private")
	);
	assert_eq!(
		normalise_path_to_unix(
			Path::new("\\\\?\\E:\\_temp\\folder_to_watch\\public\\.hgignore"),
			false
		),
		PathBuf::from("/_temp/folder_to_watch/public/.hgignore")
	);
	assert_eq!(
		normalise_path_to_unix(
			Path::new("\\\\?\\E:\\_temp\\folder_to_watch\\public\\.hgignore"),
			true
		),
		PathBuf::from("\\\\?\\E:/_temp/folder_to_watch/public/.hgignore")
	);
}
