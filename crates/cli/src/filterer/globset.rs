use std::{
	ffi::{OsString},
	path::MAIN_SEPARATOR,
	sync::Arc,
};

use miette::{IntoDiagnostic, Result};
use tracing::info;
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

	let filters = args.filter_patterns.iter()
		.map(|f| (f.to_owned(), Some(workdir.clone())));

	ignores.extend(
		args.ignore_patterns.iter()
			.map(|f| (f.to_owned(), Some(workdir.clone()))),
	);

	// TODO: bring split and strip into args
	let exts = args.filter_extensions
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

		self.inner.check_event(event, priority)
	}
}
