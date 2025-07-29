use std::{
	ffi::OsString,
	path::{Path, PathBuf, MAIN_SEPARATOR},
	sync::Arc,
};

use miette::{IntoDiagnostic, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{info, trace, trace_span};
use watchexec::{error::RuntimeError, filter::Filterer};
use watchexec_events::{
	filekind::{FileEventKind, ModifyKind},
	Event, Priority, Tag,
};
use watchexec_filterer_globset::GlobsetFilterer;

use crate::args::{filtering::FsEvent, Args};

pub mod parse;
mod proglib;
mod progs;
mod syncval;

/// A custom filterer that combines the library's Globset filterer and a switch for --no-meta
#[derive(Debug)]
pub struct WatchexecFilterer {
	inner: GlobsetFilterer,
	fs_events: Vec<FsEvent>,
	progs: Option<progs::FilterProgs>,
}

impl Filterer for WatchexecFilterer {
	#[tracing::instrument(level = "trace", skip(self))]
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

				trace!(allowed=?self.fs_events, this=?normalised, "check against fs event filter");
				if !self.fs_events.contains(&normalised) {
					return Ok(false);
				}
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
		let project_origin = args.filtering.project_origin.clone().unwrap();
		let workdir = args.command.workdir.clone().unwrap();

		let ignore_files = if args.filtering.no_discover_ignore {
			Vec::new()
		} else {
			let vcs_types = crate::dirs::vcs_types(&project_origin).await;
			crate::dirs::ignores(args, &vcs_types).await?
		};

		let mut ignores = Vec::new();

		if !args.filtering.no_default_ignore {
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

		let whitelist = args
			.filtering
			.paths
			.iter()
			.map(std::convert::Into::into)
			.filter(|p: &PathBuf| p.is_file());

		let mut filters = args
			.filtering
			.filter_patterns
			.iter()
			.map(|f| (f.to_owned(), Some(workdir.clone())))
			.collect::<Vec<_>>();

		// Instead, we should ignore everything in the directory if
		// it wasn't being watched
		filters.extend(
			args.filtering
				.paths
				.iter()
				.map(std::convert::Into::into)
				.filter(|p: &PathBuf| p.is_file())
				.filter_map(|p: PathBuf| p.to_str().map(|s| (s.into(), Some(workdir.clone())))),
		);

		for filter_file in &args.filtering.filter_files {
			filters.extend(read_filter_file(filter_file).await?);
		}

		ignores.extend(
			args.filtering
				.ignore_patterns
				.iter()
				.map(|f| (f.to_owned(), Some(workdir.clone()))),
		);

		let exts = args
			.filtering
			.filter_extensions
			.iter()
			.map(|e| OsString::from(e.strip_prefix('.').unwrap_or(e)));

		info!("initialising Globset filterer");
		Ok(Arc::new(Self {
			inner: GlobsetFilterer::new(
				project_origin,
				filters,
				ignores,
				whitelist,
				ignore_files,
				exts,
			)
			.await
			.into_diagnostic()?,
			fs_events: args.filtering.filter_fs_events.clone(),
			progs: if args.filtering.filter_programs_parsed.is_empty() {
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

	let metadata_len = file
		.metadata()
		.await
		.map(|m| usize::try_from(m.len()))
		.unwrap_or(Ok(0))
		.into_diagnostic()?;
	let filter_capacity = if metadata_len == 0 {
		0
	} else {
		metadata_len / 20
	};
	let mut filters = Vec::with_capacity(filter_capacity);

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
