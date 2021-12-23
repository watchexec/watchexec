use std::sync::Arc;

use clap::ArgMatches;
use miette::Result;
use tracing::debug;
use watchexec::filter::tagged::{Filter, TaggedFilterer};

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

	// TODO: load global/env filter files
	// TODO: load -F filter files

	Ok(filterer)
}
