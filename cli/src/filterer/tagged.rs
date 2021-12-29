use std::{path::PathBuf, sync::Arc};

use clap::ArgMatches;
use futures::future::try_join_all;
use miette::Result;
use tracing::debug;
use watchexec::{
	filter::tagged::{
		files::{self, FilterFile},
		Filter, TaggedFilterer,
	},
	ignore_files::IgnoreFile,
};

pub async fn tagged(args: &ArgMatches<'static>) -> Result<Arc<TaggedFilterer>> {
	let (project_origin, workdir) = super::common::dirs(args).await?;
	let ignores = super::common::ignores(args, &project_origin).await?;

	let mut filters = Vec::new();

	for filter in args.values_of("filter").unwrap_or_default() {
		let mut filter: Filter = filter.parse()?;
		filter.in_path = Some(workdir.clone());
		filters.push(filter);
	}

	debug!(?filters, "parsed filters");

	let filterer = TaggedFilterer::new(project_origin, workdir)?;

	filterer.add_filters(&filters).await?;

	for ignore in &ignores {
		filterer.add_ignore_file(ignore).await?;
	}

	let mut filter_files = Vec::new();
	for path in args.values_of_os("filter-file").unwrap_or_default() {
		let file = FilterFile(IgnoreFile {
			applies_in: None,
			applies_to: None,
			path: PathBuf::from(path),
		});
		filter_files.push(file);
	}

	if !args.is_present("no-global-filters") {
		// TODO: handle errors
		let (global_filter_files, _errors) = files::from_environment().await;
		filter_files.extend(global_filter_files);
	}

	let filters = try_join_all(
		filter_files
			.into_iter()
			.map(|file| async move { file.load().await }),
	)
	.await?
	.into_iter()
	.flatten()
	.collect::<Vec<_>>();
	filterer.add_filters(&filters).await?;

	Ok(filterer)
}
