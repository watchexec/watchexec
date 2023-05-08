use std::path::{Path, PathBuf};

use futures::stream::{FuturesUnordered, StreamExt};
use ignore::{
	gitignore::{Gitignore, GitignoreBuilder, Glob},
	Match,
};
use radix_trie::{Trie, TrieCommon};
use tokio::fs::read_to_string;
use tracing::{trace, trace_span};

use crate::{Error, IgnoreFile};

#[derive(Clone, Debug)]
struct Ignore {
	gitignore: Gitignore,
	builder: Option<GitignoreBuilder>,
}

/// A mutable filter dedicated to ignore files and trees of ignore files.
///
/// This reads and compiles ignore files, and should be used for handling ignore files. It's created
/// with a project origin and a list of ignore files, and new ignore files can be added later
/// (unless [`finish`](IgnoreFilter::finish()) is called).
#[derive(Clone, Debug)]
pub struct IgnoreFilter {
	origin: PathBuf,
	ignores: Trie<String, Ignore>,
}

impl IgnoreFilter {
	/// Create a new empty filterer.
	///
	/// Prefer [`new()`](IgnoreFilter::new()) if you have ignore files ready to use.
	pub fn empty(origin: impl AsRef<Path>) -> Self {
		let origin = origin.as_ref();

		let mut ignores = Trie::new();
		ignores.insert(
			origin.display().to_string(),
			Ignore {
				gitignore: Gitignore::empty(),
				builder: Some(GitignoreBuilder::new(origin)),
			},
		);

		Self {
			origin: origin.to_owned(),
			ignores,
		}
	}

	/// Read ignore files from disk and load them for filtering.
	///
	/// Use [`empty()`](IgnoreFilter::empty()) if you want an empty filterer,
	/// or to construct one outside an async environment.
	pub async fn new(origin: impl AsRef<Path> + Send, files: &[IgnoreFile]) -> Result<Self, Error> {
		let origin = dunce::simplified(origin.as_ref());
		let _span = trace_span!("build_filterer", ?origin);

		trace!(files=%files.len(), "loading file contents");
		let (files_contents, errors): (Vec<_>, Vec<_>) = files
			.iter()
			.map(|file| async move {
				trace!(?file, "loading ignore file");
				let content = read_to_string(&file.path)
					.await
					.map_err(|err| Error::Read {
						file: file.path.clone(),
						err,
					})?;
				Ok((file.clone(), content))
			})
			.collect::<FuturesUnordered<_>>()
			.collect::<Vec<_>>()
			.await
			.into_iter()
			.map(|res| match res {
				Ok(o) => (Some(o), None),
				Err(e) => (None, Some(e)),
			})
			.unzip();

		let errors: Vec<Error> = errors.into_iter().flatten().collect();
		if !errors.is_empty() {
			trace!("found {} errors", errors.len());
			return Err(Error::Multi(errors));
		}

		// TODO: different parser/adapter for non-git-syntax ignore files?

		trace!(files=%files_contents.len(), "building ignore list");

		let mut ignores_trie = Trie::new();

		// add builder for the root of the file system, so that we can handle global ignores and globs
		ignores_trie.insert(
			prefix(origin),
			Ignore {
				gitignore: Gitignore::empty(),
				builder: Some(GitignoreBuilder::new(origin)),
			},
		);

		let mut total_num_ignores = 0;
		let mut total_num_whitelists = 0;

		for (file, content) in files_contents.into_iter().flatten() {
			let _span = trace_span!("loading ignore file", ?file).entered();

			let applies_in = get_applies_in_path(origin, Some(&file));

			let parent_ignore = ignores_trie
				.get_ancestor_value(&applies_in.display().to_string())
				// unwrap will always succeed because we created an entry with the root of the origin
				.unwrap();

			let mut builder = parent_ignore
				.builder
				.clone()
				.unwrap_or_else(|| GitignoreBuilder::new(&applies_in));

			for line in content.lines() {
				if line.is_empty() || line.starts_with('#') {
					continue;
				}

				trace!(?line, "adding ignore line");
				builder
					.add_line(Some(applies_in.clone().to_owned()), line)
					.map_err(|err| Error::Glob {
						file: Some(file.path.clone()),
						err,
					})?;
			}
			trace!("compiling globset");
			let compiled_builder = builder
				.build()
				.map_err(|err| Error::Glob { file: None, err })?;

			total_num_ignores += compiled_builder.num_ignores();
			total_num_whitelists += compiled_builder.num_whitelists();

			ignores_trie.insert(
				applies_in.display().to_string(),
				Ignore {
					gitignore: compiled_builder,
					builder: Some(builder),
				},
			);
		}

		trace!(
			files=%files.len(),
			trie=?ignores_trie,
			ignores=%total_num_ignores,
			allows=%total_num_whitelists,
			"ignore files loaded and compiled",
		);

		Ok(Self {
			origin: origin.to_owned(),
			ignores: ignores_trie,
		})
	}

	/// Returns the number of ignores and allowlists loaded.
	#[must_use]
	pub fn num_ignores(&self) -> (u64, u64) {
		self.ignores.iter().fold((0, 0), |mut acc, (_, ignore)| {
			acc.0 += ignore.gitignore.num_ignores();
			acc.1 += ignore.gitignore.num_whitelists();
			acc
		})
	}

	/// Deletes the internal builder, to save memory.
	///
	/// This makes it impossible to add new ignore files without re-compiling the whole set.
	pub fn finish(&mut self) {
		let keys = self.ignores.keys().cloned().collect::<Vec<_>>();
		for key in keys {
			if let Some(ignore) = self.ignores.get_mut(&key) {
				ignore.builder = None;
			}
		}
	}

	/// Reads and adds an ignore file, if the builder is available.
	///
	/// Does nothing silently otherwise.
	pub async fn add_file(&mut self, file: &IgnoreFile) -> Result<(), Error> {
		let applies_in = &get_applies_in_path(&self.origin, Some(file))
			.display()
			.to_string();

		let Some(Ignore { builder: Some(ref mut builder), ..}) = self.ignores.get_mut(applies_in) else {
			return Ok(());
		};

		trace!(?file, "reading ignore file");
		let content = read_to_string(&file.path)
			.await
			.map_err(|err| Error::Read {
				file: file.path.clone(),
				err,
			})?;

		let _span = trace_span!("loading ignore file", ?file).entered();
		for line in content.lines() {
			if line.is_empty() || line.starts_with('#') {
				continue;
			}

			trace!(?line, "adding ignore line");
			builder
				.add_line(file.applies_in.clone(), line)
				.map_err(|err| Error::Glob {
					file: Some(file.path.clone()),
					err,
				})?;
		}

		self.recompile(Some(file))?;

		Ok(())
	}

	fn recompile(&mut self, file: Option<&IgnoreFile>) -> Result<(), Error> {
		let applies_in = get_applies_in_path(&self.origin, file)
			.display()
			.to_string();

		let Some(Ignore { gitignore: compiled, builder: Some(builder)}) = self.ignores.get(&applies_in) else {
			return Ok(());
		};

		let pre_ignores = compiled.num_ignores();
		let pre_allows = compiled.num_whitelists();

		trace!("recompiling globset");
		let recompiled = builder.build().map_err(|err| Error::Glob {
			file: file.map(|file| file.path.clone()),
			err,
		})?;

		trace!(
			new_ignores=%(recompiled.num_ignores() - pre_ignores),
			new_allows=%(recompiled.num_whitelists() - pre_allows),
			"ignore file loaded and set recompiled",
		);

		self.ignores.insert(
			applies_in,
			Ignore {
				gitignore: recompiled,
				builder: Some(builder.to_owned()),
			},
		);

		Ok(())
	}

	/// Adds some globs manually, if the builder is available.
	///
	/// Does nothing silently otherwise.
	pub fn add_globs(&mut self, globs: &[&str], applies_in: Option<&PathBuf>) -> Result<(), Error> {
		let applies_in = applies_in.unwrap_or(&self.origin);

		let Some(Ignore {builder: Some(builder), ..}) = self.ignores.get_mut(&applies_in.display().to_string()) else {
			return Ok(());
		};

		let _span = trace_span!("loading ignore globs", ?globs).entered();
		for line in globs {
			if line.is_empty() || line.starts_with('#') {
				continue;
			}

			trace!(?line, "adding ignore line");
			builder
				.add_line(Some(applies_in.clone()), line)
				.map_err(|err| Error::Glob { file: None, err })?;
		}

		self.recompile(None)?;

		Ok(())
	}

	/// Match a particular path against the ignore set.
	pub fn match_path(&self, path: &Path, is_dir: bool) -> Match<&Glob> {
		let path = dunce::simplified(path);

		let Some(ignores) = self.ignores.get_ancestor_value(&path.display().to_string()) else {
			trace!(?path, "no ignores for path");
			return Match::None;
		};

		if path.strip_prefix(&self.origin).is_ok() {
			trace!("checking against path or parents");
			ignores.gitignore.matched_path_or_any_parents(path, is_dir)
		} else {
			trace!("checking against path only");
			ignores.gitignore.matched(path, is_dir)
		}
	}

	/// Check a particular folder path against the ignore set.
	///
	/// Returns `false` if the folder should be ignored.
	///
	/// Note that this is a slightly different implementation than watchexec's Filterer trait, as
	/// the latter handles events with multiple associated paths.
	pub fn check_dir(&self, path: &Path) -> bool {
		let _span = trace_span!("check_dir", ?path).entered();

		trace!("checking against compiled ignore files");
		match self.match_path(path, true) {
			Match::None => {
				trace!("no match (pass)");
				true
			}
			Match::Ignore(glob) => {
				if glob.from().map_or(true, |f| path.strip_prefix(f).is_ok()) {
					trace!(?glob, "positive match (fail)");
					false
				} else {
					trace!(?glob, "positive match, but not in scope (pass)");
					true
				}
			}
			Match::Whitelist(glob) => {
				trace!(?glob, "negative match (pass)");
				true
			}
		}
	}
}

fn get_applies_in_path(origin: &Path, ignore_file: Option<&IgnoreFile>) -> PathBuf {
	let root_path = PathBuf::from(prefix(origin));
	if let Some(ignore_file) = ignore_file {
		ignore_file
			.applies_in
			.as_ref()
			.map(|p| PathBuf::from(dunce::simplified(p)))
			.unwrap_or(root_path)
	} else {
		root_path
	}
}

/// Gets the root component of a given path.
///
/// This will be `/` on unix systems, or a Drive letter (`C:`, `D:`, etc)
fn prefix<T: AsRef<Path>>(path: T) -> String {
	let path = path.as_ref();

	let Some(prefix) = path.components().next() else {
		return "/".into();
	};

	match prefix {
		std::path::Component::Prefix(prefix_component) => {
			prefix_component.as_os_str().to_str().unwrap_or("/").into()
		}
		_ => "/".into(),
	}
}
