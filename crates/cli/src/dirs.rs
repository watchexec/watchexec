use std::{
	collections::HashSet,
	path::{Path, PathBuf},
};

use ignore_files::{IgnoreFile, IgnoreFilesFromOriginArgs};
use miette::{miette, IntoDiagnostic, Result};
use project_origins::ProjectType;
use tokio::fs::canonicalize;
use tracing::{debug, info, warn};
use watchexec::paths::common_prefix;

use crate::args::Args;

pub async fn project_origin(args: &Args) -> Result<PathBuf> {
	let project_origin = if let Some(origin) = &args.project_origin {
		debug!(?origin, "project origin override");
		canonicalize(origin).await.into_diagnostic()?
	} else {
		let homedir = match dirs::home_dir() {
			None => None,
			Some(dir) => Some(canonicalize(dir).await.into_diagnostic()?),
		};
		debug!(?homedir, "home directory");

		let homedir_requested = homedir.as_ref().map_or(false, |home| {
			args.paths
				.binary_search_by_key(home, |w| PathBuf::from(w.clone()))
				.is_ok()
		});
		debug!(
			?homedir_requested,
			"resolved whether the homedir is explicitly requested"
		);

		let mut origins = HashSet::new();
		for path in &args.paths {
			origins.extend(project_origins::origins(path).await);
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
			origins.insert(args.workdir.clone().unwrap());
		}

		debug!(?origins, "resolved all project origins");

		// This canonicalize is probably redundant
		canonicalize(
			common_prefix(&origins)
				.ok_or_else(|| miette!("no common prefix, but this should never fail"))?,
		)
		.await
		.into_diagnostic()?
	};
	info!(?project_origin, "resolved common/project origin");

	Ok(project_origin)
}

pub async fn vcs_types(origin: &Path) -> Vec<ProjectType> {
	let vcs_types = project_origins::types(origin)
		.await
		.into_iter()
		.filter(|pt| pt.is_vcs())
		.collect::<Vec<_>>();
	info!(?vcs_types, "resolved vcs types");
	vcs_types
}

pub async fn ignores(args: &Args, vcs_types: &[ProjectType]) -> Result<Vec<IgnoreFile>> {
	let origin = args.project_origin.clone().unwrap();
	let mut skip_git_global_excludes = false;

	let mut ignores = if args.no_project_ignore {
		Vec::new()
	} else {
		let ignore_files = args.ignore_files.iter().map(|path| {
			if path.is_absolute() {
				path.into()
			} else {
				origin.join(path)
			}
		});

		let (mut ignores, errors) = ignore_files::from_origin(
			IgnoreFilesFromOriginArgs::new_unchecked(
				&origin,
				args.paths.iter().map(PathBuf::from),
				ignore_files,
			)
			.canonicalise()
			.await
			.into_diagnostic()?,
		)
		.await;

		for err in errors {
			warn!("while discovering project-local ignore files: {}", err);
		}
		debug!(?ignores, "discovered ignore files from project origin");

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

		ignores
	};

	let global_ignores = if args.no_global_ignore {
		Vec::new()
	} else {
		let (mut global_ignores, errors) = ignore_files::from_environment(Some("watchexec")).await;
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

		global_ignores
	};

	ignores.extend(global_ignores.into_iter().filter(|ig| match ig.applies_to {
		Some(pt) if pt.is_vcs() => vcs_types.contains(&pt),
		_ => true,
	}));
	debug!(
		?ignores,
		?vcs_types,
		"combined and applied overall vcs filter over ignores"
	);

	ignores.extend(args.ignore_files.iter().map(|ig| IgnoreFile {
		applies_to: None,
		applies_in: None,
		path: ig.clone(),
	}));
	debug!(
		?ignores,
		?args.ignore_files,
		"combined with ignore files from command line / env"
	);

	if args.no_project_ignore {
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

	if args.no_global_ignore {
		ignores = ignores
			.into_iter()
			.filter(|ig| ig.applies_in.is_some())
			.collect::<Vec<_>>();
		debug!(?ignores, "filtered ignores to exclude global ignores");
	}

	if args.no_vcs_ignore {
		ignores = ignores
			.into_iter()
			.filter(|ig| ig.applies_to.is_none())
			.collect::<Vec<_>>();
		debug!(?ignores, "filtered ignores to exclude VCS-specific ignores");
	}

	info!(files=?ignores.iter().map(|ig| ig.path.as_path()).collect::<Vec<_>>(), "found some ignores");
	Ok(ignores)
}
