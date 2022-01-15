use std::{
	env,
	io::{Error, ErrorKind},
	path::{Path, PathBuf},
};

use futures::{pin_mut, Stream, StreamExt};
use tokio::fs::{metadata, read_dir};
use tracing::{trace, trace_span};

use crate::project::ProjectType;

/// An ignore file.
///
/// This records both the path to the ignore file and some basic metadata about it: which project
/// type it applies to if any, and which subtree it applies in if any (`None` = global ignore file).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IgnoreFile {
	/// The path to the ignore file.
	pub path: PathBuf,

	/// The path to the subtree the ignore file applies to, or `None` for global ignores.
	pub applies_in: Option<PathBuf>,

	/// Which project type the ignore file applies to, or was found through.
	pub applies_to: Option<ProjectType>,
}

/// Finds all ignore files in the given directory and subdirectories.
///
/// This considers:
/// - Git ignore files (`.gitignore`)
/// - Mercurial ignore files (`.hgignore`)
/// - Tool-generic `.ignore` files
/// - `.git/info/exclude` files in the `path` directory only
/// - Git configurable project ignore files (with `core.excludesFile` in `.git/config`)
///
/// Importantly, this should be called from the origin of the project, not a subfolder. This
/// function will not discover the project origin, and will not traverse parent directories. Use the
/// [`project::origins`](crate::project::origins) function for that.
///
/// This function also does not distinguish between project folder types, and collects all files for
/// all supported VCSs and other project types. Use the `applies_to` field to filter the results.
///
/// All errors (permissions, etc) are collected and returned alongside the ignore files: you may
/// want to show them to the user while still using whatever ignores were successfully found. Errors
/// from files not being found are silently ignored (the files are just not returned).
///
/// ## Special case: project-local git config specifying `core.excludesFile`
///
/// If the project's `.git/config` specifies a value for `core.excludesFile`, this function will
/// return an `IgnoreFile { path: path/to/that/file, applies_in: None, applies_to: Some(ProjectType::Git) }`.
/// This is the only case in which the `applies_in` field is None from this function. When such is
/// received the global Git ignore files found by [`from_environment()`] **should be ignored**.
pub async fn from_origin(path: impl AsRef<Path>) -> (Vec<IgnoreFile>, Vec<Error>) {
	let base = path.as_ref().to_owned();
	let mut files = Vec::new();
	let mut errors = Vec::new();

	match find_file(base.join(".git/config")).await {
		Err(err) => errors.push(err),
		Ok(None) => {}
		Ok(Some(path)) => match git2::Config::open(&path) {
			Err(err) => errors.push(Error::new(ErrorKind::Other, err)),
			Ok(config) => {
				if let Ok(excludes) = config.get_path("core.excludesFile") {
					discover_file(
						&mut files,
						&mut errors,
						None,
						Some(ProjectType::Git),
						excludes,
					)
					.await;
				}
			}
		},
	}

	// TODO: integrate ignore::Filter

	discover_file(
		&mut files,
		&mut errors,
		Some(base.clone()),
		Some(ProjectType::Bazaar),
		base.join(".bzrignore"),
	)
	.await;

	discover_file(
		&mut files,
		&mut errors,
		Some(base.clone()),
		Some(ProjectType::Darcs),
		base.join("_darcs/prefs/boring"),
	)
	.await;

	discover_file(
		&mut files,
		&mut errors,
		Some(base.clone()),
		Some(ProjectType::Fossil),
		base.join(".fossil-settings/ignore-glob"),
	)
	.await;

	discover_file(
		&mut files,
		&mut errors,
		Some(base.clone()),
		Some(ProjectType::Git),
		base.join(".git/info/exclude"),
	)
	.await;

	let dirs = all_dirs(base);
	pin_mut!(dirs);
	while let Some(p) = dirs.next().await {
		match p {
			Err(err) => errors.push(err),
			Ok(dir) => {
				discover_file(
					&mut files,
					&mut errors,
					Some(dir.clone()),
					None,
					dir.join(".ignore"),
				)
				.await;

				discover_file(
					&mut files,
					&mut errors,
					Some(dir.clone()),
					Some(ProjectType::Git),
					dir.join(".gitignore"),
				)
				.await;

				discover_file(
					&mut files,
					&mut errors,
					Some(dir.clone()),
					Some(ProjectType::Mercurial),
					dir.join(".hgignore"),
				)
				.await;
			}
		}
	}

	(files, errors)
}

/// Finds all ignore files that apply to the current runtime.
///
/// This considers:
/// - User-specific git ignore files (e.g. `~/.gitignore`)
/// - Git configurable ignore files (e.g. with `core.excludesFile` in system or user config)
/// - `$XDG_CONFIG_HOME/watchexec/ignore`, as well as other locations (APPDATA on Windows…)
/// - Files from the `WATCHEXEC_IGNORE_FILES` environment variable (comma-separated)
///
/// All errors (permissions, etc) are collected and returned alongside the ignore files: you may
/// want to show them to the user while still using whatever ignores were successfully found. Errors
/// from files not being found are silently ignored (the files are just not returned).
pub async fn from_environment() -> (Vec<IgnoreFile>, Vec<Error>) {
	let mut files = Vec::new();
	let mut errors = Vec::new();

	for path in env::var("WATCHEXEC_IGNORE_FILES")
		.unwrap_or_default()
		.split(',')
	{
		discover_file(&mut files, &mut errors, None, None, PathBuf::from(path)).await;
	}

	let mut found_git_global = false;
	match git2::Config::open_default() {
		Err(err) => errors.push(Error::new(ErrorKind::Other, err)),
		Ok(config) => {
			if let Ok(excludes) = config.get_path("core.excludesFile") {
				if discover_file(
					&mut files,
					&mut errors,
					None,
					Some(ProjectType::Git),
					excludes,
				)
				.await
				{
					found_git_global = true;
				}
			}
		}
	}

	if !found_git_global {
		let mut tries = Vec::with_capacity(5);
		if let Ok(home) = env::var("XDG_CONFIG_HOME") {
			tries.push(Path::new(&home).join("git/ignore"));
		}
		if let Ok(home) = env::var("APPDATA") {
			tries.push(Path::new(&home).join(".gitignore"));
		}
		if let Ok(home) = env::var("USERPROFILE") {
			tries.push(Path::new(&home).join(".gitignore"));
		}
		if let Ok(home) = env::var("HOME") {
			tries.push(Path::new(&home).join(".config/git/ignore"));
			tries.push(Path::new(&home).join(".gitignore"));
		}

		for path in tries {
			if discover_file(&mut files, &mut errors, None, Some(ProjectType::Git), path).await {
				break;
			}
		}
	}

	let mut bzrs = Vec::with_capacity(5);
	if let Ok(home) = env::var("APPDATA") {
		bzrs.push(Path::new(&home).join("Bazzar/2.0/ignore"));
	}
	if let Ok(home) = env::var("HOME") {
		bzrs.push(Path::new(&home).join(".bazarr/ignore"));
	}

	for path in bzrs {
		if discover_file(
			&mut files,
			&mut errors,
			None,
			Some(ProjectType::Bazaar),
			path,
		)
		.await
		{
			break;
		}
	}

	let mut wgis = Vec::with_capacity(5);
	if let Ok(home) = env::var("XDG_CONFIG_HOME") {
		wgis.push(Path::new(&home).join("watchexec/ignore"));
	}
	if let Ok(home) = env::var("APPDATA") {
		wgis.push(Path::new(&home).join("watchexec/ignore"));
	}
	if let Ok(home) = env::var("USERPROFILE") {
		wgis.push(Path::new(&home).join(".watchexec/ignore"));
	}
	if let Ok(home) = env::var("HOME") {
		wgis.push(Path::new(&home).join(".watchexec/ignore"));
	}

	for path in wgis {
		if discover_file(&mut files, &mut errors, None, None, path).await {
			break;
		}
	}

	(files, errors)
}

#[inline]
pub(crate) async fn discover_file(
	files: &mut Vec<IgnoreFile>,
	errors: &mut Vec<Error>,
	applies_in: Option<PathBuf>,
	applies_to: Option<ProjectType>,
	path: PathBuf,
) -> bool {
	let _span = trace_span!("discover_file", ?path, ?applies_in, ?applies_to).entered();
	match find_file(path).await {
		Err(err) => {
			trace!(?err, "found an error");
			errors.push(err);
			false
		}
		Ok(None) => {
			trace!("found nothing");
			false
		}
		Ok(Some(path)) => {
			trace!(?path, "found a file");
			files.push(IgnoreFile {
				path,
				applies_in,
				applies_to,
			});
			true
		}
	}
}

async fn find_file(path: PathBuf) -> Result<Option<PathBuf>, Error> {
	match metadata(&path).await {
		Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
		Err(err) => Err(err),
		Ok(meta) if meta.is_file() && meta.len() > 0 => Ok(Some(path)),
		Ok(_) => Ok(None),
	}
}

fn all_dirs(path: PathBuf) -> impl Stream<Item = Result<PathBuf, Error>> {
	async_stream::try_stream! {
		yield path.clone();
		let mut to_visit = vec![path];

		while let Some(path) = to_visit.pop() {
			let mut dir = read_dir(&path).await?;
			while let Some(entry) = dir.next_entry().await? {
				if entry.file_type().await?.is_dir() {
					let path = entry.path();
					to_visit.push(path.clone());
					yield path;
				}
			}
		}
	}
}
