use std::{
	collections::HashSet,
	env,
	path::{Path, PathBuf},
	sync::Arc,
};

use clap::ArgMatches;
use dunce::canonicalize;
use miette::{IntoDiagnostic, Result};
use tracing::{debug, warn};
use watchexec::{
	filter::tagged::{Filter, Matcher, Op, Pattern, Regex, TaggedFilterer},
	ignore_files::{self, IgnoreFile},
	paths::common_prefix,
	project::{self, ProjectType},
};

pub async fn dirs(args: &ArgMatches<'static>) -> Result<(PathBuf, PathBuf)> {
	let mut origins = HashSet::new();
	for path in args.values_of("paths").unwrap_or_default().into_iter() {
		let path = canonicalize(path).into_diagnostic()?;
		origins.extend(project::origins(&path).await);
	}

	debug!(?origins, "resolved all project origins");

	let project_origin = common_prefix(&origins).unwrap_or_else(|| PathBuf::from("."));
	debug!(?project_origin, "resolved common/project origin");

	let workdir = env::current_dir()
		.and_then(|wd| wd.canonicalize())
		.into_diagnostic()?;
	debug!(?workdir, "resolved working directory");

	Ok((project_origin, workdir))
}

pub async fn ignores(args: &ArgMatches<'static>, origin: &Path) -> Result<Vec<IgnoreFile>> {
	let vcs_types = project::types(origin)
		.await
		.into_iter()
		.filter(|pt| pt.is_vcs())
		.collect::<Vec<_>>();
	debug!(?vcs_types, "resolved vcs types");

	let (mut ignores, _errors) = ignore_files::from_origin(origin).await;
	// TODO: handle errors
	debug!(?ignores, "discovered ignore files from project origin");

	let mut skip_git_global_excludes = false;
	if !vcs_types.is_empty() {
		ignores = ignores
			.into_iter()
			.filter(|ig| match ig.applies_to {
				Some(pt) if pt.is_vcs() => vcs_types.contains(&pt),
				_ => true,
			})
			.inspect(|ig| {
				if let IgnoreFile {
					applies_to: Some(ProjectType::Git),
					applies_in: None,
					..
				} = ig
				{
					warn!("project git config overrides the global excludes");
					skip_git_global_excludes = true;
				}
			})
			.collect::<Vec<_>>();
		debug!(?ignores, "filtered ignores to only those for project vcs");
		// TODO: use drain_ignore when that stabilises
	}

	let (mut global_ignores, _errors) = ignore_files::from_environment().await;
	// TODO: handle errors
	debug!(?global_ignores, "discovered ignore files from environment");

	if skip_git_global_excludes {
		global_ignores = global_ignores
			.into_iter()
			.filter(|gig| {
				!matches!(
					gig,
					IgnoreFile {
						applies_to: Some(ProjectType::Git),
						applies_in: None,
						..
					}
				)
			})
			.collect::<Vec<_>>();
		debug!(
			?global_ignores,
			"filtered global ignores to exclude global git ignores"
		);
		// TODO: use drain_ignore when that stabilises
	}

	if !vcs_types.is_empty() {
		ignores.extend(global_ignores.into_iter().filter(|ig| match ig.applies_to {
			Some(pt) if pt.is_vcs() => vcs_types.contains(&pt),
			_ => true,
		}));
		debug!(?ignores, "combined and applied final filter over ignores");
	}

	Ok(ignores)
}
