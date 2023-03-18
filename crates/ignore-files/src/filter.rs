use std::path::{Path, PathBuf};

use futures::stream::{FuturesUnordered, StreamExt};
use ignore::{
	gitignore::{Gitignore, GitignoreBuilder, Glob},
	Match,
};
use tokio::fs::read_to_string;
use tracing::{trace, trace_span};

use crate::{Error, IgnoreFile};

/// A mutable filter dedicated to ignore files and trees of ignore files.
///
/// This reads and compiles ignore files, and should be used for handling ignore files. It's created
/// with a project origin and a list of ignore files, and new ignore files can be added later
/// (unless [`finish`](IgnoreFilter::finish()) is called).
#[derive(Clone, Debug)]
pub struct IgnoreFilter {
	origin: PathBuf,
	builder: Option<GitignoreBuilder>,
	compiled: Gitignore,
}

impl IgnoreFilter {
	/// Create a new empty filterer.
	///
	/// Prefer [`new()`](IgnoreFilter::new()) if you have ignore files ready to use.
	pub fn empty(origin: impl AsRef<Path>) -> Self {
		let origin = origin.as_ref();
		Self {
			builder: Some(GitignoreBuilder::new(origin)),
			origin: origin.to_owned(),
			compiled: Gitignore::empty(),
		}
	}

	/// Read ignore files from disk and load them for filtering.
	///
	/// Use [`empty()`](IgnoreFilter::empty()) if you want an empty filterer,
	/// or to construct one outside an async environment.
	pub async fn new(origin: impl AsRef<Path> + Send, files: &[IgnoreFile]) -> Result<Self, Error> {
		let origin = origin.as_ref();
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
		let mut builder = GitignoreBuilder::new(origin);
		for (file, content) in files_contents.into_iter().flatten() {
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
		}

		trace!("compiling globset");
		let compiled = builder
			.build()
			.map_err(|err| Error::Glob { file: None, err })?;

		trace!(
			files=%files.len(),
			ignores=%compiled.num_ignores(),
			allows=%compiled.num_whitelists(),
			"ignore files loaded and compiled",
		);

		Ok(Self {
			origin: origin.to_owned(),
			builder: Some(builder),
			compiled,
		})
	}

	/// Returns the number of ignores and allowlists loaded.
	#[must_use]
	pub fn num_ignores(&self) -> (u64, u64) {
		(self.compiled.num_ignores(), self.compiled.num_whitelists())
	}

	/// Deletes the internal builder, to save memory.
	///
	/// This makes it impossible to add new ignore files without re-compiling the whole set.
	pub fn finish(&mut self) {
		self.builder = None;
	}

	/// Reads and adds an ignore file, if the builder is available.
	///
	/// Does nothing silently otherwise.
	pub async fn add_file(&mut self, file: &IgnoreFile) -> Result<(), Error> {
		if let Some(ref mut builder) = self.builder {
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

			self.recompile(file.path.clone())?;
		}

		Ok(())
	}

	fn recompile(&mut self, file: PathBuf) -> Result<(), Error> {
		if let Some(builder) = &mut self.builder {
			let pre_ignores = self.compiled.num_ignores();
			let pre_allows = self.compiled.num_whitelists();

			trace!("recompiling globset");
			let recompiled = builder.build().map_err(|err| Error::Glob {
				file: Some(file),
				err,
			})?;

			trace!(
				new_ignores=%(recompiled.num_ignores() - pre_ignores),
				new_allows=%(recompiled.num_whitelists() - pre_allows),
				"ignore file loaded and set recompiled",
			);
			self.compiled = recompiled;
		}

		Ok(())
	}

	/// Adds some globs manually, if the builder is available.
	///
	/// Does nothing silently otherwise.
	pub fn add_globs(&mut self, globs: &[&str], applies_in: Option<&PathBuf>) -> Result<(), Error> {
		if let Some(ref mut builder) = self.builder {
			let _span = trace_span!("loading ignore globs", ?globs).entered();
			for line in globs {
				if line.is_empty() || line.starts_with('#') {
					continue;
				}

				trace!(?line, "adding ignore line");
				builder
					.add_line(applies_in.cloned(), line)
					.map_err(|err| Error::Glob { file: None, err })?;
			}

			self.recompile("manual glob".into())?;
		}

		Ok(())
	}

	/// Match a particular path against the ignore set.
	pub fn match_path(&self, path: &Path, is_dir: bool) -> Match<&Glob> {
		if path.strip_prefix(&self.origin).is_ok() {
			trace!("checking against path or parents");
			self.compiled.matched_path_or_any_parents(path, is_dir)
		} else {
			trace!("checking against path only");
			self.compiled.matched(path, is_dir)
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
