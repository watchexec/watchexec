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
	fn context(self, context: impl std::fmt::Display) -> Result<T, String>;
}

impl<T, E> ErrString<T> for Result<T, E>
where
	E: std::fmt::Display,
{
	fn err_string(self) -> Result<T, String> {
		self.map_err(|e| e.to_string())
	}

	fn context(self, context: impl std::fmt::Display) -> Result<T, String> {
		self.map_err(|err| format!("{context}: {err}"))
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
		let repo = git::discover().context("discover git repo")?;
		let hash = git::head(&repo).context("read HEAD")?;
		let commit_file = git::read_commit(&repo, &hash).context("read commit data")?;
		let timestamp = git::commit_datetime(&commit_file).context("parse commit date")?;

		Ok(Self {
			git_root: repo.canonicalize().err_string()?,
			git_hash: hash.to_hex().to_string(),
			git_shorthash: hash.to_hex_with_len(7).to_string(),
			git_date: timestamp.format(DATE_FORMAT).err_string()?,
			git_datetime: timestamp.format(DATETIME_FORMAT).err_string()?,
		})
	}
}

#[cfg(feature = "git")]
mod git {
	use std::{
		fs::{read_to_string, File},
		io::Read,
		path::{Path, PathBuf},
		str::FromStr,
	};

	use flate2::bufread::ZlibDecoder;
	use gix_features::{fs::WalkDir, zlib::Inflate};
	use gix_hash::{oid, ObjectId};
	use gix_pack::{
		data::{File as PackFile, Offset},
		index::File as IndexFile,
	};
	use time::OffsetDateTime;

	use super::ErrString;

	pub fn discover() -> Result<PathBuf, String> {
		let mut current = PathBuf::from(".").canonicalize().err_string()?;
		while let Some(parent) = current.parent() {
			current = parent.into();
			if current.join(".git").exists() {
				return Ok(current.join(".git"));
			}
		}

		Err(String::from("git repo not found"))
	}

	pub fn head(repo: &Path) -> Result<ObjectId, String> {
		let headfile = repo.join("HEAD");
		let headfile = read_to_string(&headfile).context(headfile.display())?;

		let (_, headref) = headfile
			.split_once(": ")
			.ok_or_else(|| String::from("invalid .git/HEAD"))?;
		let refpath = repo.join(headref.trim());
		let reffile = read_to_string(&refpath).context(refpath.display())?;
		let reffile = reffile.trim().to_string();

		oid::try_from_bytes(
			&hex::decode(&reffile)
				.context(reffile)
				.context(refpath.display())?,
		)
		.context("parse oid")
		.map(|o| o.to_owned())
	}

	fn object_file(repo: &Path, object: &oid) -> Option<PathBuf> {
		let path = repo
			.join("objects")
			.join(format!("{:02x}", object.first_byte()))
			.join(hex::encode(&object.as_bytes()[1..]));
		if path.exists() {
			Some(path)
		} else {
			None
		}
	}

	fn find_in_index(repo: &Path, object: &oid) -> Result<Option<(PathBuf, Offset)>, String> {
		for file in WalkDir::new(repo.join("objects/pack")) {
			let file = file.context("walk objects/pack")?;

			if !file.file_type().is_file() {
				continue;
			}

			if !file.path().extension().map_or(false, |ext| ext == "idx") {
				continue;
			}

			let index = IndexFile::at(file.path(), Default::default())
				.context(file.path().display())
				.context("read index file")?;

			if let Some(i) = index.lookup(object) {
				let offset = index.pack_offset_at_index(i);
				let mut packpath = repo.join(file.path());
				packpath.set_extension("pack");
				return Ok(Some((packpath, offset)));
			}
		}

		Ok(None)
	}

	fn unpack_commit(path: &Path, offset: Offset) -> Result<Vec<u8>, String> {
		let pack_file = PackFile::at(path, Default::default())
			.context(path.display())
			.context("read pack file")?;

		let entry = pack_file.entry(offset);
		let mut buf = Vec::with_capacity(entry.decompressed_size as _);
		let mut flate = Inflate::default();
		pack_file
			.decompress_entry(&entry, &mut flate, &mut buf)
			.context(offset)
			.context("decompress commit at offset")
			.context(path.display())?;

		Ok(buf)
	}

	pub fn read_commit(repo: &Path, hash: &oid) -> Result<Vec<u8>, String> {
		if let Some(path) = object_file(repo, hash) {
			let mut file = File::open(&path)
				.context(path.display())
				.context("open file")?;

			let size = file
				.metadata()
				.err_string()
				.context(path.display())
				.context("stat file")?
				.len() as usize;
			let mut raw = Vec::with_capacity(size);
			file.read_to_end(&mut raw)
				.err_string()
				.context(path.display())
				.context("read file")?;

			let mut flate = ZlibDecoder::new(&*raw);
			let mut buf = Vec::new();
			flate
				.read_to_end(&mut buf)
				.err_string()
				.context(path.display())
				.context("inflate file")?;
			Ok(buf)
		} else {
			let (pack_path, offset) = find_in_index(repo, hash)?.ok_or_else(|| {
				String::from("HEAD is a packed ref, but can't find it in git pack")
			})?;

			unpack_commit(&pack_path, offset)
		}
	}

	pub fn commit_datetime(commit: &[u8]) -> Result<OffsetDateTime, String> {
		const COMMITTER: &[u8] = b"\ncommitter ";
		let (committer_line_offset, _) = commit
			.windows(COMMITTER.len())
			.enumerate()
			.find(|(_, bytes)| *bytes == COMMITTER)
			.ok_or_else(|| String::from("HEAD has no committer"))?;
		let date_line = &commit[(committer_line_offset + 1)..]
			.split(|b| *b == b'\n')
			.next()
			.ok_or_else(|| String::from("splitting line failed"))?;
		let date_s = &date_line
			.rsplit(|b| *b == b' ')
			.nth(1)
			.ok_or_else(|| String::from("malformed committer in commit"))
			.and_then(|s| std::str::from_utf8(s).err_string())
			.and_then(|s| i64::from_str(s).err_string())?;

		OffsetDateTime::from_unix_timestamp(*date_s).err_string()
	}
}
