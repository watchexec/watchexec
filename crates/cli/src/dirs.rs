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

use crate::args::{command::CommandArgs, filtering::FilteringArgs, Args};

pub async fn project_origin(
	FilteringArgs {
		project_origin,
		recursive_paths,
		non_recursive_paths,
		..
	}: &FilteringArgs,
	CommandArgs { workdir, .. }: &CommandArgs,
) -> Result<PathBuf> {
	let project_origin = if let Some(origin) = project_origin {
		debug!(?origin, "project origin override");
		canonicalize(origin).await.into_diagnostic()?
	} else {
		let homedir = match dirs::home_dir() {
			None => None,
			Some(dir) => Some(canonicalize(dir).await.into_diagnostic()?),
		};
		debug!(?homedir, "home directory");

		// The watched paths aren't resolved yet at this point (that needs the origin), so do it
		// here against the workdir, without canonicalising, so that missing paths aren't fatal.
		let paths = watch_candidates(recursive_paths, non_recursive_paths, workdir.as_deref());
		debug!(?paths, "candidate paths for origin discovery");

		let homedir_requested = homedir
			.as_ref()
			.map_or(false, |home| paths.iter().any(|path| path == home));
		debug!(
			?homedir_requested,
			"resolved whether the homedir is explicitly requested"
		);

		let mut origins = HashSet::new();
		for path in &paths {
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
			origins.insert(workdir.clone().unwrap());
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
	debug!(?project_origin, "resolved common/project origin");

	Ok(project_origin)
}

/// Resolves the paths given with `-w` / `-W` against the workdir, for use as starting points of
/// origin discovery. Relative paths are joined onto the workdir; the `/dev/null` sentinel (which
/// means "watch nothing") is skipped. If nothing is left, the workdir itself is the only candidate.
fn watch_candidates(
	recursive_paths: &[PathBuf],
	non_recursive_paths: &[PathBuf],
	workdir: Option<&Path>,
) -> Vec<PathBuf> {
	let mut paths: Vec<PathBuf> = recursive_paths
		.iter()
		.chain(non_recursive_paths)
		.filter(|path| path.as_path() != Path::new("/dev/null"))
		.map(|path| {
			if path.is_absolute() {
				path.clone()
			} else {
				workdir.map_or_else(|| path.clone(), |wd| wd.join(path))
			}
		})
		.collect();

	if paths.is_empty() {
		if let Some(workdir) = workdir {
			paths.push(workdir.to_owned());
		}
	}

	paths
}

pub async fn vcs_types(origin: &Path) -> Vec<ProjectType> {
	let vcs_types = project_origins::types(origin)
		.await
		.into_iter()
		.filter(|pt| pt.is_vcs())
		.collect::<Vec<_>>();
	info!(?vcs_types, "effective vcs types");
	vcs_types
}

pub async fn ignores(args: &Args, vcs_types: &[ProjectType]) -> Result<Vec<IgnoreFile>> {
	let origin = args.filtering.project_origin.clone().unwrap();
	let mut skip_git_global_excludes = false;

	let mut ignores = if args.filtering.no_project_ignore {
		Vec::new()
	} else {
		let ignore_files = args.filtering.ignore_files.iter().map(|path| {
			if path.is_absolute() {
				path.into()
			} else {
				origin.join(path)
			}
		});

		let (mut ignores, errors) = ignore_files::from_origin(
			IgnoreFilesFromOriginArgs::new_unchecked(
				&origin,
				args.filtering.paths.iter().map(PathBuf::from),
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

	let global_ignores = if args.filtering.no_global_ignore {
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

	ignores.extend(args.filtering.ignore_files.iter().map(|ig| IgnoreFile {
		applies_to: None,
		applies_in: None,
		path: ig.clone(),
	}));
	debug!(
		?ignores,
		?args.filtering.ignore_files,
		"combined with ignore files from command line / env"
	);

	if args.filtering.no_project_ignore {
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

	if args.filtering.no_global_ignore {
		ignores = ignores
			.into_iter()
			.filter(|ig| ig.applies_in.is_some())
			.collect::<Vec<_>>();
		debug!(?ignores, "filtered ignores to exclude global ignores");
	}

	if args.filtering.no_vcs_ignore {
		ignores = ignores
			.into_iter()
			.filter(|ig| ig.applies_to.is_none())
			.collect::<Vec<_>>();
		debug!(?ignores, "filtered ignores to exclude VCS-specific ignores");
	}

	info!(files=?ignores.iter().map(|ig| ig.path.as_path()).collect::<Vec<_>>(), "found some ignores");
	Ok(ignores)
}

#[cfg(test)]
mod tests {
	use clap::Parser;

	use super::*;
	use crate::args::Args;

	fn args_in(workdir: &Path, extra: &[&str]) -> Args {
		let mut args = Args::parse_from(
			std::iter::once("watchexec")
				.chain(extra.iter().copied())
				.chain(std::iter::once("true")),
		);
		args.command.workdir = Some(workdir.to_owned());
		args
	}

	async fn origin_in(workdir: &Path, extra: &[&str]) -> PathBuf {
		let args = args_in(workdir, extra);
		project_origin(&args.filtering, &args.command)
			.await
			.expect("origin should resolve")
	}

	/// The origin has to be discovered by walking up from the watched paths (or the workdir when
	/// there are none), so running from anywhere inside a project must find the same origin, not
	/// whichever directory happens to be current.
	#[tokio::test]
	async fn discovers_the_same_origin_from_anywhere_in_the_project() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path().join("project");
		let deep = root.join("deep");
		let sub = deep.join("sub");
		std::fs::create_dir_all(&sub).expect("create dirs");
		std::fs::write(root.join("Cargo.toml"), "").expect("write marker");

		let from_root = origin_in(&root, &[]).await;
		let from_deep = origin_in(&deep, &[]).await;
		let from_sub = origin_in(&sub, &[]).await;

		assert_eq!(
			from_deep, from_root,
			"origin from {deep:?} should be the same as from {root:?}"
		);
		assert_eq!(
			from_sub, from_root,
			"origin from {sub:?} should be the same as from {root:?}"
		);
		assert_ne!(
			from_sub, sub,
			"origin should not just be the current directory"
		);
	}

	/// Same, but for a path given with '-w' rather than the workdir.
	#[tokio::test]
	async fn discovers_the_origin_of_a_watched_path() {
		let tmp = tempfile::tempdir().expect("tempdir");
		let root = tmp.path().join("project");
		let sub = root.join("deep").join("sub");
		let outside = tmp.path().join("outside");
		std::fs::create_dir_all(&sub).expect("create dirs");
		std::fs::create_dir_all(&outside).expect("create dirs");
		std::fs::write(root.join("Cargo.toml"), "").expect("write marker");

		let watched = origin_in(&outside, &["-w", sub.to_str().expect("utf-8 path")]).await;
		assert_eq!(
			watched,
			origin_in(&root, &[]).await,
			"origin of watched {sub:?} should be the project origin"
		);
	}

	#[test]
	fn watch_candidates_resolve_against_the_workdir() {
		let workdir = Path::new("/work/dir");
		assert_eq!(
			watch_candidates(
				&[PathBuf::from("rel"), PathBuf::from("/abs")],
				&[PathBuf::from("nonrec")],
				Some(workdir),
			),
			vec![
				PathBuf::from("/work/dir/rel"),
				PathBuf::from("/abs"),
				PathBuf::from("/work/dir/nonrec"),
			]
		);
	}

	#[test]
	fn watch_candidates_fall_back_to_the_workdir() {
		let workdir = Path::new("/work/dir");
		assert_eq!(
			watch_candidates(&[], &[], Some(workdir)),
			vec![PathBuf::from("/work/dir")]
		);
		assert_eq!(
			watch_candidates(&[PathBuf::from("/dev/null")], &[], Some(workdir)),
			vec![PathBuf::from("/work/dir")],
			"/dev/null means watch nothing, so it isn't a discovery starting point"
		);
	}
}
