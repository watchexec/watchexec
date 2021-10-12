use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use dunce::canonicalize;
use tokio::fs::read_to_string;
use tracing::{debug, trace, warn};
use unicase::UniCase;

use crate::error::RuntimeError;
use crate::event::{Event, Tag};
use crate::filter::tagged::error::TaggedFiltererError;
use crate::filter::Filterer;
use crate::ignore_files::IgnoreFile;

// to make filters
pub use globset::Glob;
pub use regex::Regex;

pub mod error;
mod parse;
pub mod swaplock;

#[derive(Debug)]
pub struct TaggedFilterer {
	/// The directory the project is in, its origin.
	///
	/// This is used to resolve absolute paths without an `in_path` context.
	origin: PathBuf,

	/// Where the program is running from.
	///
	/// This is used to resolve relative paths without an `in_path` context.
	workdir: PathBuf,

	/// All filters that are applied, in order, by matcher.
	filters: swaplock::SwapLock<HashMap<Matcher, Vec<Filter>>>,
}

impl Filterer for TaggedFilterer {
	fn check_event(&self, event: &Event) -> Result<bool, RuntimeError> {
		self.check(event).map_err(|e| e.into())
	}
}

impl TaggedFilterer {
	fn check(&self, event: &Event) -> Result<bool, TaggedFiltererError> {
		// TODO: trace logging
		if self.filters.borrow().is_empty() {
			trace!("no filters, skipping entire check (pass)");
			return Ok(true);
		}

		trace!(tags=%event.tags.len(), "checking all tags on the event");
		for tag in &event.tags {
			let filters = self.filters.borrow().get(&tag.into()).cloned();
			if let Some(tag_filters) = filters {
				trace!(?tag, "checking tag");

				if tag_filters.is_empty() {
					trace!(?tag, "no filters for this tag, skipping (pass)");
					continue;
				}

				trace!(?tag, filters=%tag_filters.len(), "found some filters for this tag");

				let mut tag_match = true;
				for filter in &tag_filters {
					trace!(?filter, ?tag, "checking filter againt tag");
					if let Some(app) = self.match_tag(filter, tag)? {
						if filter.negate {
							if app {
								trace!(prev=%tag_match, now=%true, "negate filter passes, resetting tag to pass");
								tag_match = true;
							} else {
								trace!(prev=%tag_match, now=%tag_match, "negate filter fails, ignoring");
							}
						} else {
							trace!(prev=%tag_match, this=%app, now=%(tag_match&app), "filter applies to this tag");
							tag_match &= app;
						}
					}
				}

				if !tag_match {
					trace!(?tag, "tag fails check, failing entire event");
					return Ok(false);
				}

				trace!(?tag, "tag passes check, continuing");
			} else {
				trace!(?tag, "no filters for this tag, skipping (pass)");
			}
		}

		trace!(?event, "passing event");
		Ok(true)
	}
}

impl TaggedFilterer {
	pub fn new(
		origin: impl Into<PathBuf>,
		workdir: impl Into<PathBuf>,
	) -> Result<Arc<Self>, TaggedFiltererError> {
		// TODO: make it criticalerror
		Ok(Arc::new(Self {
			origin: canonicalize(origin.into())?,
			workdir: canonicalize(workdir.into())?,
			filters: swaplock::SwapLock::new(HashMap::new()),
		}))
	}

	// filter ctx              event path                filter                 outcome
	// /foo/bar                /foo/bar/baz.txt          baz.txt                pass
	// /foo/bar                /foo/bar/baz.txt          /baz.txt               pass
	// /foo/bar                /foo/bar/baz.txt          /baz.*                 pass
	// /foo/bar                /foo/bar/baz.txt          /blah                  fail
	// /foo/quz                /foo/bar/baz.txt          /baz.*                 skip
	// TODO: lots of tests

	// Ok(Some(bool)) => the match was applied, bool is the result
	// Ok(None) => for some precondition, the match was not done (mismatched tag, out of context, …)
	fn match_tag(&self, filter: &Filter, tag: &Tag) -> Result<Option<bool>, TaggedFiltererError> {
		trace!(?tag, matcher=?filter.on, "matching filter to tag");
		match (tag, filter.on) {
			(tag, Matcher::Tag) => filter.matches(tag.discriminant_name()),
			(Tag::Path(path), Matcher::Path) => {
				let resolved = if let Some(ctx) = &filter.in_path {
					if let Ok(suffix) = path.strip_prefix(ctx) {
						suffix.strip_prefix("/").unwrap_or(suffix)
					} else {
						return Ok(None);
					}
				} else if let Ok(suffix) = path.strip_prefix(&self.workdir) {
					suffix.strip_prefix("/").unwrap_or(suffix)
				} else if let Ok(suffix) = path.strip_prefix(&self.origin) {
					suffix.strip_prefix("/").unwrap_or(suffix)
				} else {
					path.strip_prefix("/").unwrap_or(path)
				};

				trace!(?resolved, "resolved path to match filter against");
				filter.matches(resolved.to_string_lossy())
			}
			(Tag::FileEventKind(kind), Matcher::FileEventKind) => {
				filter.matches(format!("{:?}", kind))
			}
			(Tag::Source(src), Matcher::Source) => filter.matches(src.to_string()),
			(Tag::Process(pid), Matcher::Process) => filter.matches(pid.to_string()),
			(Tag::Signal(_sig), Matcher::Signal) => todo!("tagged filterer: signal matcher"),
			(Tag::ProcessCompletion(_oes), Matcher::ProcessCompletion) => {
				todo!("tagged filterer: completion matcher")
			}
			(tag, matcher) => {
				trace!(?tag, ?matcher, "no match for tag, skipping");
				return Ok(None);
			}
		}
		.map(Some)
	}

	pub async fn add_filters(&self, filters: &[Filter]) -> Result<(), TaggedFiltererError> {
		debug!(?filters, "adding filters to filterer");

		let mut recompile_globs = false;
		let mut recompile_not_globs = false;

		let filters = filters
			.iter()
			.cloned()
			.inspect(|f| match f.op {
				Op::Glob => {
					recompile_globs = true;
				}
				Op::NotGlob => {
					recompile_not_globs = true;
				}
				_ => {}
			})
			.map(Filter::canonicalised)
			.collect::<Result<Vec<_>, _>>()?;
		// TODO: use miette's related and issue canonicalisation errors for all of them

		self.filters
			.change(|fs| {
				for filter in filters {
					fs.entry(filter.on).or_default().push(filter);
				}
			})
			.await
			.map_err(|err| error::TaggedFiltererError::FilterChange { action: "add", err })?;

		if recompile_globs {
			self.recompile_globs(Op::Glob).await?;
		}

		if recompile_not_globs {
			self.recompile_globs(Op::NotGlob).await?;
		}

		Ok(())
	}

	async fn recompile_globs(&self, op_filter: Op) -> Result<(), error::TaggedFiltererError> {
		todo!()

		// globs:
		// - use ignore's impl
		// - after adding some filters, recompile by making a gitignorebuilder and storing the gitignore
		// - use two gitignores: one for NotGlob (which is used for gitignores) and one for Glob (invert results from its matches)
	}

	pub async fn add_ignore_file(&self, file: &IgnoreFile) -> Result<(), TaggedFiltererError> {
		let content = read_to_string(&file.path).await?;
		let lines = content.lines();
		let mut ignores = Vec::with_capacity(lines.size_hint().0);

		for line in lines {
			if line.is_empty() || line.starts_with('#') {
				continue;
			}

			ignores.push(Filter::from_glob_ignore(file.applies_in.clone(), line));
		}

		self.add_filters(&ignores).await
	}

	pub async fn clear_filters(&self) -> Result<(), TaggedFiltererError> {
		debug!("removing all filters from filterer");
		self.filters
			.replace(Default::default())
			.await
			.map_err(|err| TaggedFiltererError::FilterChange {
				action: "clear all",
				err,
			})?;
		Ok(())
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
	/// Path the filter applies from.
	pub in_path: Option<PathBuf>,

	/// Which tag the filter applies to.
	pub on: Matcher,

	/// The operation to perform on the tag's value.
	pub op: Op,

	/// The pattern to match against the tag's value.
	pub pat: Pattern,

	/// If true, a positive match with this filter will override negative matches from previous
	/// filters on the same tag, and negative matches will be ignored.
	pub negate: bool,
}

impl Filter {
	// TODO non-unicode matching
	pub fn matches(&self, subject: impl AsRef<str>) -> Result<bool, TaggedFiltererError> {
		let subject = subject.as_ref();

		trace!(op=?self.op, pat=?self.pat, ?subject, "performing filter match");
		Ok(match (self.op, &self.pat) {
			(Op::Equal, Pattern::Exact(pat)) => UniCase::new(subject) == UniCase::new(pat),
			(Op::NotEqual, Pattern::Exact(pat)) => UniCase::new(subject) != UniCase::new(pat),
			(Op::Regex, Pattern::Regex(pat)) => pat.is_match(subject),
			(Op::NotRegex, Pattern::Regex(pat)) => !pat.is_match(subject),
			(Op::InSet, Pattern::Set(set)) => set.contains(subject),
			(Op::InSet, Pattern::Exact(pat)) => subject == pat,
			(Op::NotInSet, Pattern::Set(set)) => !set.contains(subject),
			(Op::NotInSet, Pattern::Exact(pat)) => subject != pat,
			(Op::Glob | Op::NotGlob, Pattern::Glob(_)) => {
				panic!("globs are handled outside of Filter::matches")
			}
			(op, pat) => {
				warn!(
					"trying to match pattern {:?} with op {:?}, that cannot work",
					pat, op
				);
				false
			}
		})
	}

	pub fn from_glob_ignore(in_path: Option<PathBuf>, glob: &str) -> Self {
		let (glob, negate) = glob.strip_prefix('!').map_or((glob, false), |g| (g, true));

		Self {
			in_path,
			on: Matcher::Path,
			op: Op::NotGlob,
			pat: Pattern::Glob(glob.to_string()),
			negate,
		}
	}

	fn canonicalised(mut self) -> Result<Self, TaggedFiltererError> {
		if let Some(ctx) = self.in_path {
			self.in_path = Some(canonicalize(&ctx)?);
			trace!(canon=?ctx, "canonicalised in_path");
		}

		Ok(self)
	}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum Matcher {
	Tag,
	Path,
	FileEventKind,
	Source,
	Process,
	Signal,
	ProcessCompletion,
}

impl From<&Tag> for Matcher {
	fn from(tag: &Tag) -> Self {
		match tag {
			Tag::Path(_) => Matcher::Path,
			Tag::FileEventKind(_) => Matcher::FileEventKind,
			Tag::Source(_) => Matcher::Source,
			Tag::Process(_) => Matcher::Process,
			Tag::Signal(_) => Matcher::Signal,
			Tag::ProcessCompletion(_) => Matcher::ProcessCompletion,
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Op {
	Auto,     // =
	Equal,    // ==
	NotEqual, // !=
	Regex,    // ~=
	NotRegex, // ~!
	Glob,     // *=
	NotGlob,  // *!
	InSet,    // :=
	NotInSet, // :!
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Pattern {
	Exact(String),
	Regex(Regex),
	Glob(String),
	Set(HashSet<String>),
}

impl PartialEq<Self> for Pattern {
	fn eq(&self, other: &Self) -> bool {
		match (self, other) {
			(Self::Exact(l), Self::Exact(r)) | (Self::Glob(l), Self::Glob(r)) => l == r,
			(Self::Regex(l), Self::Regex(r)) => l.as_str() == r.as_str(),
			(Self::Set(l), Self::Set(r)) => l == r,
			_ => false,
		}
	}
}

impl Eq for Pattern {}
