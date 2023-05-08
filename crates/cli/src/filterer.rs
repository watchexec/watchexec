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

mod dirs;
mod proglib;
mod progs;
#[cfg(windows)]
mod windows_norm;

/// A custom filterer that combines the library's Globset filterer and a switch for --no-meta
#[derive(Debug)]
pub struct WatchexecFilterer {
	inner: GlobsetFilterer,
	fs_events: Vec<FsEvent>,
	progs: Option<progs::FilterProgs>,
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

		#[cfg(windows)]
		{
			let normalised = windows_norm::normalise_event_to_unix(event, false);
			trace!(event=?normalised, "check against unix-normalised event");
			if !self.inner.check_event(&normalised, priority)? {
				return Ok(false);
			}

			let prefixed_normalised = windows_norm::normalise_event_to_unix(event, true);
			trace!(event=?prefixed_normalised, "check against prefixed unix-normalised event");
			if !self.inner.check_event(&prefixed_normalised, priority)? {
				return Ok(false);
			}
		}

		trace!("check against original event");
		if !self.inner.check_event(event, priority)? {
			return Ok(false);
		}

		if let Some(progs) = &self.progs {
			trace!("check against program filters");
			if !progs.check(event)? {
				return Ok(false);
			}
		}

		Ok(true)
	}
}

impl WatchexecFilterer {
	/// Create a new filterer from the given arguments
	pub async fn new(args: &Args) -> Result<Arc<Self>> {
		let (project_origin, workdir) = dirs::dirs(args).await?;
		let vcs_types = dirs::vcs_types(&project_origin).await;
		let ignore_files = dirs::ignores(args, &vcs_types, &project_origin).await;

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
		Ok(Arc::new(Self {
			inner: GlobsetFilterer::new(project_origin, filters, ignores, ignore_files, exts)
				.await
				.into_diagnostic()?,
			fs_events: args.filter_fs_events.clone(),
			progs: if args.filter_programs.is_empty() {
				None
			} else {
				Some(progs::FilterProgs::new(args)?)
			},
		}))
	}
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
