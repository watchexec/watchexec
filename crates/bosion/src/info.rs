use std::{
	env::var,
	path::{Path, PathBuf},
};

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
				features.push(stripped.replace('_', "-").to_lowercase().clone());
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
	/// This is truncated to 8 characters.
	pub git_shorthash: String,

	/// The date of the current commit, in the format `YYYY-MM-DD`, at UTC.
	pub git_date: String,

	/// The datetime of the current commit, in the format `YYYY-MM-DD HH:MM:SS`, at UTC.
	pub git_datetime: String,
}

#[cfg(feature = "git")]
impl GitInfo {
	fn gather() -> Result<Self, String> {
		let git_root = Self::find_git_dir(Path::new("."))
			.ok_or_else(|| "no git repository found".to_string())?;

		let hash =
			Self::resolve_head(&git_root).ok_or_else(|| "could not resolve HEAD".to_string())?;

		let timestamp = Self::read_commit_timestamp(&git_root, &hash)
			.ok_or_else(|| "could not read commit timestamp".to_string())?;

		let timestamp = OffsetDateTime::from_unix_timestamp(timestamp).err_string()?;

		Ok(Self {
			git_root: git_root.canonicalize().err_string()?,
			git_shorthash: hash.chars().take(8).collect(),
			git_hash: hash,
			git_date: timestamp.format(DATE_FORMAT).err_string()?,
			git_datetime: timestamp.format(DATETIME_FORMAT).err_string()?,
		})
	}

	fn find_git_dir(start: &Path) -> Option<PathBuf> {
		use std::fs;

		let mut current = start.canonicalize().ok()?;
		loop {
			let git_dir = current.join(".git");
			if git_dir.is_dir() {
				return Some(git_dir);
			}
			// Handle git worktrees: .git can be a file containing "gitdir: <path>"
			if git_dir.is_file() {
				let content = fs::read_to_string(&git_dir).ok()?;
				if let Some(path) = content.strip_prefix("gitdir: ") {
					return Some(PathBuf::from(path.trim()));
				}
			}
			if !current.pop() {
				return None;
			}
		}
	}

	fn resolve_head(git_dir: &Path) -> Option<String> {
		use std::fs;

		let head_content = fs::read_to_string(git_dir.join("HEAD")).ok()?;
		let head_content = head_content.trim();

		if let Some(ref_path) = head_content.strip_prefix("ref: ") {
			Self::resolve_ref(git_dir, ref_path)
		} else {
			// Detached HEAD - direct commit hash
			Some(head_content.to_string())
		}
	}

	fn resolve_ref(git_dir: &Path, ref_path: &str) -> Option<String> {
		use std::fs;

		// Try loose ref first
		let ref_file = git_dir.join(ref_path);
		if let Ok(content) = fs::read_to_string(&ref_file) {
			return Some(content.trim().to_string());
		}

		// Try packed-refs
		let packed_refs = git_dir.join("packed-refs");
		if let Ok(content) = fs::read_to_string(&packed_refs) {
			for line in content.lines() {
				if line.starts_with('#') || line.starts_with('^') {
					continue;
				}
				let parts: Vec<_> = line.split_whitespace().collect();
				if parts.len() >= 2 && parts[1] == ref_path {
					return Some(parts[0].to_string());
				}
			}
		}

		None
	}

	fn read_commit_timestamp(git_dir: &Path, hash: &str) -> Option<i64> {
		// Try loose object first
		if let Some(timestamp) = Self::read_loose_commit_timestamp(git_dir, hash) {
			return Some(timestamp);
		}

		// Try packfiles
		Self::read_packed_commit_timestamp(git_dir, hash)
	}

	fn read_loose_commit_timestamp(git_dir: &Path, hash: &str) -> Option<i64> {
		use flate2::read::ZlibDecoder;
		use std::{fs, io::Read};

		let (prefix, suffix) = hash.split_at(2);
		let object_path = git_dir.join("objects").join(prefix).join(suffix);

		let compressed = fs::read(&object_path).ok()?;
		let mut decoder = ZlibDecoder::new(&compressed[..]);
		let mut decompressed = Vec::new();
		decoder.read_to_end(&mut decompressed).ok()?;

		Self::parse_commit_timestamp(&decompressed)
	}

	fn read_packed_commit_timestamp(git_dir: &Path, hash: &str) -> Option<i64> {
		use std::fs;

		let pack_dir = git_dir.join("objects").join("pack");
		let entries = fs::read_dir(&pack_dir).ok()?;

		// Parse the hash into bytes for comparison
		let hash_bytes = Self::hex_to_bytes(hash)?;

		for entry in entries.flatten() {
			let path = entry.path();
			if path.extension().and_then(|e| e.to_str()) == Some("idx") {
				if let Some(offset) = Self::find_object_in_index(&path, &hash_bytes) {
					let pack_path = path.with_extension("pack");
					if let Some(data) = Self::read_pack_object(&pack_path, offset) {
						return Self::parse_commit_timestamp(&data);
					}
				}
			}
		}

		None
	}

	fn hex_to_bytes(hex: &str) -> Option<[u8; 20]> {
		let mut bytes = [0u8; 20];
		if hex.len() != 40 {
			return None;
		}
		for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
			let s = std::str::from_utf8(chunk).ok()?;
			bytes[i] = u8::from_str_radix(s, 16).ok()?;
		}
		Some(bytes)
	}

	fn find_object_in_index(idx_path: &Path, hash: &[u8; 20]) -> Option<u64> {
		use std::{
			fs::File,
			io::{Read, Seek, SeekFrom},
		};

		let mut file = File::open(idx_path).ok()?;
		let mut header = [0u8; 8];
		file.read_exact(&mut header).ok()?;

		// Check for v2 index magic: 0xff744f63
		if header[0..4] != [0xff, 0x74, 0x4f, 0x63] {
			return None; // Only support v2 index
		}

		let version = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
		if version != 2 {
			return None;
		}

		// Read fanout table (256 * 4 bytes)
		let mut fanout = [0u32; 256];
		for entry in &mut fanout {
			let mut buf = [0u8; 4];
			file.read_exact(&mut buf).ok()?;
			*entry = u32::from_be_bytes(buf);
		}

		let total_objects = fanout[255] as usize;
		let first_byte = hash[0] as usize;

		// Find range of objects with this first byte
		let start = if first_byte == 0 {
			0
		} else {
			fanout[first_byte - 1] as usize
		};
		let end = fanout[first_byte] as usize;

		if start >= end {
			return None;
		}

		// Binary search within the hash section
		// Hashes start at offset 8 + 256*4 = 1032
		let hash_section_offset = 8 + 256 * 4;

		let mut left = start;
		let mut right = end;

		while left < right {
			let mid = left + (right - left) / 2;
			let hash_offset = hash_section_offset + mid * 20;

			file.seek(SeekFrom::Start(hash_offset as u64)).ok()?;
			let mut found_hash = [0u8; 20];
			file.read_exact(&mut found_hash).ok()?;

			match found_hash.cmp(hash) {
				std::cmp::Ordering::Equal => {
					// Found! Now get the offset
					// CRC section starts after all hashes
					// Offset section starts after CRC section
					let offset_section =
						hash_section_offset + total_objects * 20 + total_objects * 4;
					let offset_entry = offset_section + mid * 4;

					file.seek(SeekFrom::Start(offset_entry as u64)).ok()?;
					let mut offset_buf = [0u8; 4];
					file.read_exact(&mut offset_buf).ok()?;
					let offset = u32::from_be_bytes(offset_buf);

					// Check if this is a large offset (MSB set)
					if offset & 0x80000000 != 0 {
						// Large offset - need to read from 8-byte offset table
						let large_idx = (offset & 0x7fffffff) as usize;
						let large_offset_section = offset_section + total_objects * 4;
						let large_entry = large_offset_section + large_idx * 8;

						file.seek(SeekFrom::Start(large_entry as u64)).ok()?;
						let mut large_buf = [0u8; 8];
						file.read_exact(&mut large_buf).ok()?;
						return Some(u64::from_be_bytes(large_buf));
					}

					return Some(u64::from(offset));
				}
				std::cmp::Ordering::Less => left = mid + 1,
				std::cmp::Ordering::Greater => right = mid,
			}
		}

		None
	}

	fn read_pack_object(pack_path: &Path, offset: u64) -> Option<Vec<u8>> {
		use flate2::read::ZlibDecoder;
		use std::{
			fs::File,
			io::{Read, Seek, SeekFrom},
		};

		let mut file = File::open(pack_path).ok()?;
		file.seek(SeekFrom::Start(offset)).ok()?;

		// Read object header (variable length encoding)
		let mut byte = [0u8; 1];
		file.read_exact(&mut byte).ok()?;

		let obj_type = (byte[0] >> 4) & 0x07;
		let mut size = u64::from(byte[0] & 0x0f);
		let mut shift = 4;

		while byte[0] & 0x80 != 0 {
			file.read_exact(&mut byte).ok()?;
			size |= u64::from(byte[0] & 0x7f) << shift;
			shift += 7;
		}

		// Object types: 1=commit, 2=tree, 3=blob, 4=tag, 6=ofs_delta, 7=ref_delta
		match obj_type {
			1..=4 => {
				// Regular object - just decompress
				let mut decoder = ZlibDecoder::new(&mut file);
				#[allow(clippy::cast_possible_truncation)]
				let mut data = Vec::with_capacity(size as usize);
				decoder.read_to_end(&mut data).ok()?;

				// Add the git object header
				let type_name = match obj_type {
					1 => "commit",
					2 => "tree",
					3 => "blob",
					4 => "tag",
					_ => unreachable!(),
				};
				let mut result = format!("{} {}\0", type_name, data.len()).into_bytes();
				result.extend(data);
				Some(result)
			}
			6 | 7 => {
				// Delta objects - not supported for simplicity
				// In practice, the HEAD commit is often a delta, but resolving
				// deltas requires recursive lookups which adds complexity
				None
			}
			_ => None,
		}
	}

	fn parse_commit_timestamp(data: &[u8]) -> Option<i64> {
		let content = std::str::from_utf8(data).ok()?;
		// Skip the header (e.g., "commit 123\0")
		let content = content.split('\0').nth(1)?;

		for line in content.lines() {
			if let Some(rest) = line.strip_prefix("committer ") {
				// Format: "Name <email> timestamp timezone"
				let parts: Vec<_> = rest.rsplitn(3, ' ').collect();
				if parts.len() >= 2 {
					return parts[1].parse().ok();
				}
			}
		}
		None
	}
}
