use std::{
	collections::HashSet,
	env,
	path::{Path, PathBuf},
};

use clap::ArgMatches;
use dunce::canonicalize;
use miette::{IntoDiagnostic, Result};
use tracing::{debug, warn};
use watchexec::{
	ignore::{self, IgnoreFile},
	paths::common_prefix,
	project::{self, ProjectType},
};

pub async fn dirs(args: &ArgMatches<'static>) -> Result<(PathBuf, PathBuf)> {
	let mut origins = HashSet::new();
	for path in args.values_of("paths").unwrap_or_default() {
		let path = canonicalize(path).into_diagnostic()?;
		origins.extend(project::origins(&path).await);
	}

	debug!(?origins, "resolved all project origins");

	let project_origin =
		canonicalize(common_prefix(&origins).unwrap_or_else(|| PathBuf::from(".")))
			.into_diagnostic()?;
	debug!(?project_origin, "resolved common/project origin");

	let workdir = env::current_dir()
		.and_then(canonicalize)
		.into_diagnostic()?;
	debug!(?workdir, "resolved working directory");

	Ok((project_origin, workdir))
}

pub async fn vcs_types(origin: &Path) -> Vec<ProjectType> {
	let vcs_types = project::types(origin)
		.await
		.into_iter()
		.filter(|pt| pt.is_vcs())
		.collect::<Vec<_>>();
	debug!(?vcs_types, "resolved vcs types");
	vcs_types
}

pub async fn ignores(
	args: &ArgMatches<'static>,
	vcs_types: &[ProjectType],
	origin: &Path,
) -> Result<Vec<IgnoreFile>> {
	let (mut ignores, errors) = ignore::from_origin(origin).await;
	for err in errors {
		warn!("while discovering project-local ignore files: {}", err);
	}
	debug!(?ignores, "discovered ignore files from project origin");

	// TODO: use drain_ignore instead for x = x.filter()... when that stabilises

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
	}

	let (mut global_ignores, errors) = ignore::from_environment().await;
	for err in errors {
		warn!("while discovering global ignore files: {}", err);
	}
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
	}

	if !vcs_types.is_empty() {
		ignores.extend(global_ignores.into_iter().filter(|ig| match ig.applies_to {
			Some(pt) if pt.is_vcs() => vcs_types.contains(&pt),
			_ => true,
		}));
		debug!(
			?ignores,
			"combined and applied overall vcs filter over ignores"
		);
	}

	if args.is_present("no-project-ignore") {
		ignores = ignores
			.into_iter()
			.filter(|ig| {
				!ig.applies_in
					.as_ref()
					.map_or(false, |p| p.starts_with(&origin))
			})
			.collect::<Vec<_>>();
		debug!(
			?ignores,
			"filtered ignores to exclude project-local ignores"
		);
	}

	if args.is_present("no-global-ignore") {
		ignores = ignores
			.into_iter()
			.filter(|ig| !matches!(ig.applies_in, None))
			.collect::<Vec<_>>();
		debug!(?ignores, "filtered ignores to exclude global ignores");
	}

	if args.is_present("no-vcs-ignore") {
		ignores = ignores
			.into_iter()
			.filter(|ig| matches!(ig.applies_to, None))
			.collect::<Vec<_>>();
		debug!(?ignores, "filtered ignores to exclude VCS-specific ignores");
	}

	Ok(ignores)
}
