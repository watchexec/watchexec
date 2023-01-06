use std::{
	collections::HashSet,
	env,
	io::{Error, ErrorKind},
	path::{Path, PathBuf},
};

use git_config::{path::interpolate::Context as InterpolateContext, File, Path as GitPath};
use project_origins::ProjectType;
use tokio::fs::{metadata, read_dir};
use tracing::{trace, trace_span};

use crate::{IgnoreFile, IgnoreFilter};

/// The separator for paths used in environment variables.
#[cfg(unix)]
const PATH_SEPARATOR: &str = ":";
/// The separator for paths used in environment variables.
#[cfg(not(unix))]
const PATH_SEPARATOR: &str = ";";

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
		Ok(Some(path)) => match path.parent().map(File::from_git_dir) {
			None => errors.push(Error::new(
				ErrorKind::Other,
				"unreachable: .git/config must have a parent",
			)),
			Some(Err(err)) => errors.push(Error::new(ErrorKind::Other, err)),
			Some(Ok(config)) => {
				if let Ok(excludes) = config.value::<GitPath<'_>>("core", None, "excludesFile") {
					match excludes.interpolate(InterpolateContext {
						home_dir: env::var("HOME").ok().map(PathBuf::from).as_deref(),
						..Default::default()
					}) {
						Ok(e) => {
							discover_file(
								&mut files,
								&mut errors,
								None,
								Some(ProjectType::Git),
								e.into(),
							)
							.await;
						}
						Err(err) => {
							errors.push(Error::new(ErrorKind::Other, err));
						}
					}
				}
			}
		},
	}

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

	trace!("visiting child directories for ignore files");
	match DirTourist::new(&base, &files).await {
		Ok(mut dirs) => {
			loop {
				match dirs.next().await {
					Visit::Done => break,
					Visit::Skip => continue,
					Visit::Find(dir) => {
						if discover_file(
							&mut files,
							&mut errors,
							Some(dir.clone()),
							None,
							dir.join(".ignore"),
						)
						.await
						{
							dirs.add_last_file_to_filter(&mut files, &mut errors).await;
						}

						if discover_file(
							&mut files,
							&mut errors,
							Some(dir.clone()),
							Some(ProjectType::Git),
							dir.join(".gitignore"),
						)
						.await
						{
							dirs.add_last_file_to_filter(&mut files, &mut errors).await;
						}

						if discover_file(
							&mut files,
							&mut errors,
							Some(dir.clone()),
							Some(ProjectType::Mercurial),
							dir.join(".hgignore"),
						)
						.await
						{
							dirs.add_last_file_to_filter(&mut files, &mut errors).await;
						}
					}
				}
			}
			errors.extend(dirs.errors);
		}
		Err(err) => {
			errors.push(err);
		}
	}

	(files, errors)
}

/// Finds all ignore files that apply to the current runtime.
///
/// Takes an optional `appname` for the calling application for looking at an environment variable
/// and an application-specific config location.
///
/// This considers:
/// - User-specific git ignore files (e.g. `~/.gitignore`)
/// - Git configurable ignore files (e.g. with `core.excludesFile` in system or user config)
/// - `$XDG_CONFIG_HOME/{appname}/ignore`, as well as other locations (APPDATA on Windows…)
/// - Files from the `{APPNAME}_IGNORE_FILES` environment variable (separated the same was as `PATH`)
///
/// All errors (permissions, etc) are collected and returned alongside the ignore files: you may
/// want to show them to the user while still using whatever ignores were successfully found. Errors
/// from files not being found are silently ignored (the files are just not returned).
pub async fn from_environment(appname: Option<&str>) -> (Vec<IgnoreFile>, Vec<Error>) {
	let mut files = Vec::new();
	let mut errors = Vec::new();

	if let Some(name) = appname {
		for path in env::var(format!("{}_IGNORE_FILES", name.to_uppercase()))
			.unwrap_or_default()
			.split(PATH_SEPARATOR)
		{
			discover_file(&mut files, &mut errors, None, None, PathBuf::from(path)).await;
		}
	}

	let mut found_git_global = false;
	match File::from_environment_overrides().map(|mut env| {
		File::from_globals().map(move |glo| {
			env.append(glo);
			env
		})
	}) {
		Err(err) => errors.push(Error::new(ErrorKind::Other, err)),
		Ok(Err(err)) => errors.push(Error::new(ErrorKind::Other, err)),
		Ok(Ok(config)) => {
			if let Ok(excludes) = config.value::<GitPath<'_>>("core", None, "excludesFile") {
				match excludes.interpolate(InterpolateContext {
					home_dir: env::var("HOME").ok().map(PathBuf::from).as_deref(),
					..Default::default()
				}) {
					Ok(e) => {
						if discover_file(
							&mut files,
							&mut errors,
							None,
							Some(ProjectType::Git),
							e.into(),
						)
						.await
						{
							found_git_global = true;
						}
					}
					Err(err) => {
						errors.push(Error::new(ErrorKind::Other, err));
					}
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

	if let Some(name) = appname {
		let mut wgis = Vec::with_capacity(4);
		if let Ok(home) = env::var("XDG_CONFIG_HOME") {
			wgis.push(Path::new(&home).join(format!("{name}/ignore")));
		}
		if let Ok(home) = env::var("APPDATA") {
			wgis.push(Path::new(&home).join(format!("{name}/ignore")));
		}
		if let Ok(home) = env::var("USERPROFILE") {
			wgis.push(Path::new(&home).join(format!(".{name}/ignore")));
		}
		if let Ok(home) = env::var("HOME") {
			wgis.push(Path::new(&home).join(format!(".{name}/ignore")));
		}

		for path in wgis {
			if discover_file(&mut files, &mut errors, None, None, path).await {
				break;
			}
		}
	}

	(files, errors)
}

// TODO: add context to these errors

/// Utility function to handle looking for an ignore file and adding it to a list if found.
///
/// This is mostly an internal function, but it is exposed for other filterers to use.
#[inline]
pub async fn discover_file(
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

#[derive(Debug)]
struct DirTourist {
	base: PathBuf,
	to_visit: Vec<PathBuf>,
	to_skip: HashSet<PathBuf>,
	pub errors: Vec<std::io::Error>,
	filter: IgnoreFilter,
}

#[derive(Debug)]
enum Visit {
	Find(PathBuf),
	Skip,
	Done,
}

impl DirTourist {
	pub async fn new(base: &Path, files: &[IgnoreFile]) -> Result<Self, Error> {
		let base = dunce::canonicalize(base)?;
		trace!("create IgnoreFilterer for visiting directories");
		let mut filter = IgnoreFilter::new(&base, files)
			.await
			.map_err(|err| Error::new(ErrorKind::Other, err))?;

		filter
			.add_globs(
				&[
					"/.git",
					"/.hg",
					"/.bzr",
					"/_darcs",
					"/.fossil-settings",
					"/.svn",
					"/.pijul",
				],
				Some(&base),
			)
			.map_err(|err| Error::new(ErrorKind::Other, err))?;

		Ok(Self {
			to_visit: vec![base.clone()],
			base,
			to_skip: HashSet::new(),
			errors: Vec::new(),
			filter,
		})
	}

	pub async fn next(&mut self) -> Visit {
		if let Some(path) = self.to_visit.pop() {
			let _span = trace_span!("visit_path", ?path).entered();
			if self.must_skip(&path) {
				trace!("in skip list");
				return Visit::Skip;
			}

			if !self.filter.check_dir(&path) {
				trace!("path is ignored, adding to skip list");
				self.skip(path);
				return Visit::Skip;
			}

			let mut dir = match read_dir(&path).await {
				Ok(dir) => dir,
				Err(err) => {
					trace!("failed to read dir: {}", err);
					self.errors.push(err);
					return Visit::Skip;
				}
			};

			while let Some(entry) = match dir.next_entry().await {
				Ok(entry) => entry,
				Err(err) => {
					trace!("failed to read dir entries: {}", err);
					self.errors.push(err);
					return Visit::Skip;
				}
			} {
				let path = entry.path();
				let _span = trace_span!("dir_entry", ?path).entered();

				if self.must_skip(&path) {
					trace!("in skip list");
					continue;
				}

				match entry.file_type().await {
					Ok(ft) => {
						if ft.is_dir() {
							if !self.filter.check_dir(&path) {
								trace!("path is ignored, adding to skip list");
								self.skip(path);
								continue;
							}

							trace!("found a dir, adding to list");
							self.to_visit.push(path);
						} else {
							trace!("not a dir");
						}
					}
					Err(err) => {
						trace!("failed to read filetype, adding to skip list: {}", err);
						self.errors.push(err);
						self.skip(path);
					}
				}
			}

			Visit::Find(path)
		} else {
			Visit::Done
		}
	}

	pub fn skip(&mut self, path: PathBuf) {
		let check_path = path.as_path();
		self.to_visit.retain(|p| !p.starts_with(check_path));
		self.to_skip.insert(path);
	}

	pub(crate) async fn add_last_file_to_filter(
		&mut self,
		files: &mut [IgnoreFile],
		errors: &mut Vec<Error>,
	) {
		if let Some(ig) = files.last() {
			if let Err(err) = self.filter.add_file(ig).await {
				errors.push(Error::new(ErrorKind::Other, err));
			}
		}
	}

	fn must_skip(&self, mut path: &Path) -> bool {
		if self.to_skip.contains(path) {
			return true;
		}
		while let Some(parent) = path.parent() {
			if parent == self.base {
				break;
			}
			if self.to_skip.contains(parent) {
				return true;
			}
			path = parent;
		}

		false
	}
}
