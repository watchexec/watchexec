use std::sync::Arc;

use clap::ArgMatches;
use futures::future::try_join_all;
use miette::{IntoDiagnostic, Result};
use tracing::{debug, trace, warn};
use watchexec::{
	filter::tagged::{
		files::{self, FilterFile},
		Filter, Matcher, Op, Pattern, TaggedFilterer,
	},
	ignore::IgnoreFile,
	project::ProjectType,
};

pub async fn tagged(args: &ArgMatches<'static>) -> Result<Arc<TaggedFilterer>> {
	let (project_origin, workdir) = super::common::dirs(args).await?;
	let vcs_types = super::common::vcs_types(&project_origin).await;
	let ignores = super::common::ignores(args, &vcs_types, &project_origin).await?;

	let filterer = TaggedFilterer::new(project_origin, workdir.clone())?;

	for ignore in &ignores {
		filterer.add_ignore_file(ignore).await?;
	}

	let mut filter_files = Vec::new();
	for path in args.values_of_os("filter-file").unwrap_or_default() {
		let file = FilterFile(IgnoreFile {
			applies_in: None,
			applies_to: None,
			path: dunce::canonicalize(path).into_diagnostic()?,
		});
		filter_files.push(file);
	}
	debug!(?filter_files, "resolved command filter files");

	if !args.is_present("no-global-filters") {
		let (global_filter_files, errors) = files::from_environment().await;
		for err in errors {
			warn!("while discovering project-local filter files: {}", err);
		}
		debug!(?global_filter_files, "discovered global filter files");
		filter_files.extend(global_filter_files);
	}

	let mut filters = try_join_all(
		filter_files
			.into_iter()
			.map(|file| async move { file.load().await }),
	)
	.await?
	.into_iter()
	.flatten()
	.collect::<Vec<_>>();

	for filter in args.values_of("filter").unwrap_or_default() {
		let mut filter: Filter = filter.parse()?;
		filter.in_path = Some(workdir.clone());
		filters.push(filter);
	}

	if !args.is_present("no-default-ignore") {
		filters.extend([
			Filter::from_glob_ignore(None, ".DS_Store/"),
			Filter::from_glob_ignore(None, "*.py[co]"),
			Filter::from_glob_ignore(None, "#*#"),
			Filter::from_glob_ignore(None, ".#*"),
			Filter::from_glob_ignore(None, ".*.kate-swp"),
			Filter::from_glob_ignore(None, ".*.sw?"),
			Filter::from_glob_ignore(None, ".*.sw?x"),
		]);

		if vcs_types.contains(&ProjectType::Git) {
			filters.push(Filter::from_glob_ignore(None, "/.git"));
		}

		if vcs_types.contains(&ProjectType::Mercurial) {
			filters.push(Filter::from_glob_ignore(None, "/.hg"));
		}

		if vcs_types.contains(&ProjectType::Subversion) {
			filters.push(Filter::from_glob_ignore(None, "/.svn"));
		}

		if vcs_types.contains(&ProjectType::Bazaar) {
			filters.push(Filter::from_glob_ignore(None, "/.bzr"));
		}

		if vcs_types.contains(&ProjectType::Darcs) {
			filters.push(Filter::from_glob_ignore(None, "/_darcs"));
		}

		if vcs_types.contains(&ProjectType::Fossil) {
			filters.push(Filter::from_glob_ignore(None, "/.fossil-settings"));
		}

		if vcs_types.contains(&ProjectType::Pijul) {
			filters.push(Filter::from_glob_ignore(None, "/.pijul"));
		}
	}

	if args.is_present("no-meta") {
		filters.push(Filter {
			in_path: Some(workdir.clone()),
			on: Matcher::FileEventKind,
			op: Op::NotGlob,
			pat: Pattern::Glob("Modify(Metadata(*))".to_string()),
			negate: false,
		});
	}

	debug!(filters=%filters.len(), "parsed filters");
	trace!(?filters, "all filters");
	filterer.add_filters(&filters).await?;

	Ok(filterer)
}
