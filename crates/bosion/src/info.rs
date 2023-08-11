use std::{env::var, path::PathBuf};

use time::{format_description::FormatItem, macros::format_description, OffsetDateTime};

/// Gathered build-time information
///
/// This struct contains all the information gathered by `bosion`. It is not meant to be used
/// directly under normal circumstances, but is public for documentation purposes and if you wish
/// to build your own frontend for whatever reason. In that case, note that no effort has been made
/// to make this usable outside of the build.rs environment.
///
/// The `git` field is only available when the `git` feature is enabled, and if there is a git
/// repository to read from. The repository is discovered by walking up the directory tree until one
/// is found, which means workspaces or more complex monorepos are automatically supported. If there
/// are any errors reading the repository, the `git` field will be `None` and a rustc warning will
/// be printed.
#[derive(Debug, Clone)]
pub struct Info {
	/// The crate version, as read from the `CARGO_PKG_VERSION` environment variable.
	pub crate_version: String,

	/// The crate features, as found by the presence of `CARGO_FEATURE_*` environment variables.
	///
	/// These are normalised to lowercase and have underscores replaced by hyphens.
	pub crate_features: Vec<String>,

	/// The build date, in the format `YYYY-MM-DD`, at UTC.
	///
	/// This is either current as of build time, or from the timestamp specified by the
	/// `SOURCE_DATE_EPOCH` environment variable, for
	/// [reproducible builds](https://reproducible-builds.org/).
	pub build_date: String,

	/// The build datetime, in the format `YYYY-MM-DD HH:MM:SS`, at UTC.
	///
	/// This is either current as of build time, or from the timestamp specified by the
	/// `SOURCE_DATE_EPOCH` environment variable, for
	/// [reproducible builds](https://reproducible-builds.org/).
	pub build_datetime: String,

	/// Git repository information, if available.
	pub git: Option<GitInfo>,
}

trait ErrString<T> {
	fn err_string(self) -> Result<T, String>;
}

impl<T, E> ErrString<T> for Result<T, E>
where
	E: std::fmt::Display,
{
	fn err_string(self) -> Result<T, String> {
		self.map_err(|e| e.to_string())
	}
}

const DATE_FORMAT: &[FormatItem<'static>] = format_description!("[year]-[month]-[day]");
const DATETIME_FORMAT: &[FormatItem<'static>] =
	format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

impl Info {
	/// Gathers build-time information
	///
	/// This is not meant to be used directly under normal circumstances, but is public if you wish
	/// to build your own frontend for whatever reason. In that case, note that no effort has been
	/// made to make this usable outside of the build.rs environment.
	pub fn gather() -> Result<Self, String> {
		let build_date = Self::build_date()?;

		Ok(Self {
			crate_version: var("CARGO_PKG_VERSION").err_string()?,
			crate_features: Self::features(),
			build_date: build_date.format(DATE_FORMAT).err_string()?,
			build_datetime: build_date.format(DATETIME_FORMAT).err_string()?,

			#[cfg(feature = "git")]
			git: GitInfo::gather()
				.map_err(|e| {
					println!("cargo:warning=git info gathering failed: {e}");
				})
				.ok(),
			#[cfg(not(feature = "git"))]
			git: None,
		})
	}

	fn build_date() -> Result<OffsetDateTime, String> {
		if cfg!(feature = "reproducible") {
			if let Ok(date) = var("SOURCE_DATE_EPOCH") {
				if let Ok(date) = date.parse::<i64>() {
					return OffsetDateTime::from_unix_timestamp(date).err_string();
				}
			}
		}

		Ok(OffsetDateTime::now_utc())
	}

	fn features() -> Vec<String> {
		let mut features = Vec::new();

		for (key, _) in std::env::vars() {
			if let Some(stripped) = key.strip_prefix("CARGO_FEATURE_") {
				features.push(stripped.replace('_', "-").to_lowercase().to_string());
			}
		}

		features
	}

	pub(crate) fn set_reruns(&self) {
		if cfg!(feature = "reproducible") {
			println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
		}

		if let Some(git) = &self.git {
			let git_head = git.git_root.join("HEAD");
			println!("cargo:rerun-if-changed={}", git_head.display());
		}
	}
}

/// Git repository information
#[derive(Debug, Clone)]
pub struct GitInfo {
	/// The absolute path to the git repository's data folder.
	///
	/// In a normal repository, this is `.git`, _not_ the index or working directory.
	pub git_root: PathBuf,

	/// The full hash of the current commit.
	///
	/// Note that this makes no effore to handle dirty working directories, so it may not be
	/// representative of the current state of the code.
	pub git_hash: String,

	/// The short hash of the current commit.
	///
	/// This is read from git and not truncated manually, so it may be longer than 7 characters.
	pub git_shorthash: String,

	/// The date of the current commit, in the format `YYYY-MM-DD`, at UTC.
	pub git_date: String,

	/// The datetime of the current commit, in the format `YYYY-MM-DD HH:MM:SS`, at UTC.
	pub git_datetime: String,
}

#[cfg(feature = "git")]
impl GitInfo {
	fn gather() -> Result<Self, String> {
		let (path, _) = gix::discover::upwards(".").err_string()?;
		let repo = gix::discover(path).err_string()?;
		let head = repo.head_commit().err_string()?;
		let time = head.time().err_string()?;
		let timestamp = OffsetDateTime::from_unix_timestamp(time.seconds as _).err_string()?;

		Ok(Self {
			git_root: repo.path().canonicalize().err_string()?,
			git_hash: head.id().to_string(),
			git_shorthash: head.short_id().err_string()?.to_string(),
			git_date: timestamp.format(DATE_FORMAT).err_string()?,
			git_datetime: timestamp.format(DATETIME_FORMAT).err_string()?,
		})
	}
}
