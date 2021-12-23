use std::{ffi::OsString, sync::Arc};

use clap::ArgMatches;
use miette::{IntoDiagnostic, Result};
use watchexec::filter::globset::GlobsetFilterer;

pub async fn globset(args: &ArgMatches<'static>) -> Result<Arc<GlobsetFilterer>> {
	let (project_origin, _workdir) = super::common::dirs(args).await?;
	let _ignores = super::common::ignores(args, &project_origin).await?;
	// TODO: load ignorefiles

	let filters = args
		.values_of("filter")
		.unwrap_or_default()
		.map(|f| (f.to_owned(), None));
	// TODO: scope to workdir?

	// TODO: load ignores from args

	let exts = args
		.values_of("extensions")
		.unwrap_or_default()
		.map(|s| s.split(',').map(|s| OsString::from(s.trim())))
		.flatten();
	// TODO: get osstrings directly

	Ok(Arc::new(
		GlobsetFilterer::new(project_origin, filters, vec![], exts).into_diagnostic()?,
	))
}
