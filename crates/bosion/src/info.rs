use std::{env::var, path::PathBuf};

use time::{format_description::FormatItem, macros::format_description, OffsetDateTime};

#[derive(Debug, Clone)]
pub struct Info {
	pub crate_version: String,
	pub crate_features: Vec<String>,
	pub build_date: String,
	pub build_datetime: String,
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
					println!("cargo:warning=git info gathering failed: {}", e);
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

#[derive(Debug, Clone)]
pub struct GitInfo {
	pub git_root: PathBuf,
	pub git_hash: String,
	pub git_shorthash: String,
	pub git_date: String,
	pub git_datetime: String,
}

#[cfg(feature = "git")]
impl GitInfo {
	fn gather() -> Result<Self, String> {
		let (path, _) = gix::discover::upwards(".").err_string()?;
		let repo = gix::discover(path).err_string()?;
		let head = repo.head_commit().err_string()?;
		let time = head.time().err_string()?;
		let timestamp =
			OffsetDateTime::from_unix_timestamp(time.seconds_since_unix_epoch as _).err_string()?;

		Ok(Self {
			git_root: repo.path().canonicalize().err_string()?,
			git_hash: head.id().to_string(),
			git_shorthash: head.short_id().err_string()?.to_string(),
			git_date: timestamp.format(DATE_FORMAT).err_string()?,
			git_datetime: timestamp.format(DATETIME_FORMAT).err_string()?,
		})
	}
}
