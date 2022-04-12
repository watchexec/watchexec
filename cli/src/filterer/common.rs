use std::{
	collections::HashSet,
	env,
	path::{Path, PathBuf},
};

use clap::ArgMatches;
use dunce::canonicalize;
use miette::{miette, IntoDiagnostic, Result};
use tracing::{debug, warn};
use watchexec::{
	ignore::{self, IgnoreFile},
	paths::common_prefix,
	project::{self, ProjectType},
};

pub async fn dirs(args: &ArgMatches<'static>) -> Result<(PathBuf, PathBuf)> {
	let curdir = env::current_dir()
		.and_then(canonicalize)
		.into_diagnostic()?;
	debug!(?curdir, "current directory");

	let homedir = dirs::home_dir()
		.map(canonicalize)
		.transpose()
		.into_diagnostic()?;
	debug!(?homedir, "home directory");

	let mut paths = HashSet::new();
	for path in args.values_of("paths").unwrap_or_default() {
		paths.insert(canonicalize(path).into_diagnostic()?);
	}

	let homedir_requested = homedir.as_ref().map_or(false, |home| paths.contains(home));
	debug!(
		?homedir_requested,
		"resolved whether the homedir is explicitly requested"
	);

	if paths.is_empty() {
		debug!("no paths, using current directory");
		paths.insert(curdir.clone());
	}

	debug!(?paths, "resolved all watched paths");

	let mut origins = HashSet::new();
	for path in paths {
		origins.extend(project::origins(&path).await);
	}

	match (homedir, homedir_requested) {
		(Some(ref dir), false) if origins.contains(dir) => {
			debug!("removing homedir from origins");
			origins.remove(dir);
		}
		_ => {}
	}

	if origins.is_empty() {
		debug!("no origins, using current directory");
		origins.insert(curdir.clone());
	}

	debug!(?origins, "resolved all project origins");

	// This canonicalize is probably redundant
	let project_origin = canonicalize(
		common_prefix(&origins)
			.ok_or_else(|| miette!("no common prefix, but this should never fail"))?,
	)
	.into_diagnostic()?;
	debug!(?project_origin, "resolved common/project origin");

	let workdir = curdir;
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
) -> Vec<IgnoreFile> {
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

	ignores.extend(global_ignores.into_iter().filter(|ig| match ig.applies_to {
		Some(pt) if pt.is_vcs() => vcs_types.contains(&pt),
		_ => true,
	}));
	debug!(
		?ignores,
		?vcs_types,
		"combined and applied overall vcs filter over ignores"
	);

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

	ignores
}
