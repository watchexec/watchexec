use std::{
	ffi::OsString,
	path::{Path, PathBuf, MAIN_SEPARATOR},
	sync::Arc,
};

use miette::{IntoDiagnostic, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{info, trace, trace_span};
use watchexec::{
	error::RuntimeError,
	event::{
		filekind::{FileEventKind, ModifyKind},
		Event, Priority, Tag,
	},
	filter::Filterer,
};
use watchexec_filterer_globset::GlobsetFilterer;

use crate::args::{Args, FsEvent};

pub async fn globset(args: &Args) -> Result<Arc<WatchexecFilterer>> {
	let (project_origin, workdir) = super::common::dirs(args).await?;
	let vcs_types = super::common::vcs_types(&project_origin).await;
	let ignore_files = super::common::ignores(args, &vcs_types, &project_origin).await;

	let mut ignores = Vec::new();

	if !args.no_default_ignore {
		ignores.extend([
			(format!("**{MAIN_SEPARATOR}.DS_Store"), None),
			(String::from("watchexec.*.log"), None),
			(String::from("*.py[co]"), None),
			(String::from("#*#"), None),
			(String::from(".#*"), None),
			(String::from(".*.kate-swp"), None),
			(String::from(".*.sw?"), None),
			(String::from(".*.sw?x"), None),
			(format!("**{MAIN_SEPARATOR}.bzr{MAIN_SEPARATOR}**"), None),
			(format!("**{MAIN_SEPARATOR}_darcs{MAIN_SEPARATOR}**"), None),
			(
				format!("**{MAIN_SEPARATOR}.fossil-settings{MAIN_SEPARATOR}**"),
				None,
			),
			(format!("**{MAIN_SEPARATOR}.git{MAIN_SEPARATOR}**"), None),
			(format!("**{MAIN_SEPARATOR}.hg{MAIN_SEPARATOR}**"), None),
			(format!("**{MAIN_SEPARATOR}.pijul{MAIN_SEPARATOR}**"), None),
			(format!("**{MAIN_SEPARATOR}.svn{MAIN_SEPARATOR}**"), None),
		]);
	}

	let mut filters = args
		.filter_patterns
		.iter()
		.map(|f| (f.to_owned(), Some(workdir.clone())))
		.collect::<Vec<_>>();

	for filter_file in &args.filter_files {
		filters.extend(read_filter_file(filter_file).await?);
	}

	ignores.extend(
		args.ignore_patterns
			.iter()
			.map(|f| (f.to_owned(), Some(workdir.clone()))),
	);

	let exts = args
		.filter_extensions
		.iter()
		.map(|e| OsString::from(e.strip_prefix('.').unwrap_or(e)));

	info!("initialising Globset filterer");
	Ok(Arc::new(WatchexecFilterer {
		inner: GlobsetFilterer::new(project_origin, filters, ignores, ignore_files, exts)
			.await
			.into_diagnostic()?,
		fs_events: args.filter_fs_events.clone(),
	}))
}

async fn read_filter_file(path: &Path) -> Result<Vec<(String, Option<PathBuf>)>> {
	let _span = trace_span!("loading filter file", ?path).entered();

	let file = tokio::fs::File::open(path).await.into_diagnostic()?;

	let mut filters =
		Vec::with_capacity(file.metadata().await.map(|m| m.len() as usize).unwrap_or(0) / 20);

	let reader = BufReader::new(file);
	let mut lines = reader.lines();
	while let Some(line) = lines.next_line().await.into_diagnostic()? {
		let line = line.trim();
		if line.is_empty() || line.starts_with('#') {
			continue;
		}

		trace!(?line, "adding filter line");
		filters.push((line.to_owned(), Some(path.to_owned())));
	}

	Ok(filters)
}

/// A custom filterer that combines the library's Globset filterer and a switch for --no-meta
#[derive(Debug)]
pub struct WatchexecFilterer {
	inner: GlobsetFilterer,
	fs_events: Vec<FsEvent>,
}

impl Filterer for WatchexecFilterer {
	fn check_event(&self, event: &Event, priority: Priority) -> Result<bool, RuntimeError> {
		for tag in &event.tags {
			if let Tag::FileEventKind(fek) = tag {
				let normalised = match fek {
					FileEventKind::Access(_) => FsEvent::Access,
					FileEventKind::Modify(ModifyKind::Name(_)) => FsEvent::Rename,
					FileEventKind::Modify(ModifyKind::Metadata(_)) => FsEvent::Metadata,
					FileEventKind::Modify(_) => FsEvent::Modify,
					FileEventKind::Create(_) => FsEvent::Create,
					FileEventKind::Remove(_) => FsEvent::Remove,
					_ => continue,
				};

				if !self.fs_events.contains(&normalised) {
					return Ok(false);
				}
			}
		}

		trace!("check against original event");
		if !self.inner.check_event(event, priority)? {
			return Ok(false);
		}

		Ok(true)
	}
}

#[cfg(windows)]
mod windows_norm {
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
}
