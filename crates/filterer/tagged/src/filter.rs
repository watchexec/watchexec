use std::collections::HashSet;
use std::path::PathBuf;

use globset::Glob;
use regex::Regex;
use tokio::fs::canonicalize;
use tracing::{trace, warn};
use unicase::UniCase;
use watchexec::event::Tag;

use crate::TaggedFiltererError;

/// A tagged filter.
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
	/// Matches the filter against a subject.
	///
	/// This is really an internal method to the tagged filterer machinery, exposed so you can build
	/// your own filterer using the same types or the textual syntax. As such its behaviour is not
	/// guaranteed to be stable (its signature is, though).
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
			(op @ (Op::Glob | Op::NotGlob), Pattern::Glob(glob)) => {
				// FIXME: someway that isn't this horrible
				match Glob::new(glob) {
					Ok(glob) => {
						let matches = glob.compile_matcher().is_match(subject);
						match op {
							Op::Glob => matches,
							Op::NotGlob => !matches,
							_ => unreachable!(),
						}
					}
					Err(err) => {
						warn!(
							"failed to compile glob for non-path match, skipping (pass): {}",
							err
						);
						true
					}
				}
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

	/// Create a filter from a gitignore-style glob pattern.
	///
	/// The optional path is for the `in_path` field of the filter. When parsing gitignore files, it
	/// should be set to the path of the _directory_ the ignore file is in.
	///
	/// The resulting filter matches on [`Path`][Matcher::Path], with the [`NotGlob`][Op::NotGlob]
	/// op, and a [`Glob`][Pattern::Glob] pattern. If it starts with a `!`, it is negated.
	#[must_use]
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

	/// Returns the filter with its `in_path` canonicalised.
	pub async fn canonicalised(mut self) -> Result<Self, TaggedFiltererError> {
		if let Some(ctx) = self.in_path {
			self.in_path =
				Some(
					canonicalize(&ctx)
						.await
						.map_err(|err| TaggedFiltererError::IoError {
							about: "canonicalise Filter in_path",
							err,
						})?,
				);
			trace!(canon=?ctx, "canonicalised in_path");
		}

		Ok(self)
	}
}

/// What a filter matches on.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum Matcher {
	/// The presence of a tag on an event.
	Tag,

	/// A path in a filesystem event. Paths are always canonicalised.
	///
	/// Note that there may be multiple paths in an event (e.g. both source and destination for renames), and filters
	/// will be matched on all of them.
	Path,

	/// The file type of an object in a filesystem event.
	///
	/// This is not guaranteed to be present for every filesystem event.
	///
	/// It can be any of these values: `file`, `dir`, `symlink`, `other`. That last one means
	/// "not any of the first three."
	FileType,

	/// The [`EventKind`][notify::event::EventKind] of a filesystem event.
	///
	/// This is the Debug representation of the event kind. Examples:
	/// - `Access(Close(Write))`
	/// - `Modify(Data(Any))`
	/// - `Modify(Metadata(Permissions))`
	/// - `Remove(Folder)`
	///
	/// You should probably use globs or regexes to match these, ex:
	/// - `Create(*)`
	/// - `Modify\(Name\(.+`
	FileEventKind,

	/// The [event source][crate::event::Source] the event came from.
	///
	/// These are the lowercase names of the variants.
	Source,

	/// The ID of the process which caused the event.
	///
	/// Note that it's rare for events to carry this information.
	Process,

	/// A signal sent to the main process.
	///
	/// This can be matched both on the signal number as an integer, and on the signal name as a
	/// string. On Windows, only `BREAK` is supported; `CTRL_C` parses but won't work. Matching is
	/// on both uppercase and lowercase forms.
	///
	/// Interrupt signals (`TERM` and `INT` on Unix, `CTRL_C` on Windows) are parsed, but these are
	/// marked Urgent internally to Watchexec, and thus bypass filtering entirely.
	Signal,

	/// The exit status of a subprocess.
	///
	/// This is only present for events issued when the subprocess exits. The value is matched on
	/// both the exit code as an integer, and either `success` or `fail`, whichever succeeds.
	ProcessCompletion,

	/// The [`Priority`] of the event.
	///
	/// This is never `urgent`, as urgent events bypass filtering.
	Priority,
}

impl Matcher {
	pub(crate) fn from_tag(tag: &Tag) -> &'static [Self] {
		match tag {
			Tag::Path {
				file_type: None, ..
			} => &[Self::Path],
			Tag::Path { .. } => &[Self::Path, Self::FileType],
			Tag::FileEventKind(_) => &[Self::FileEventKind],
			Tag::Source(_) => &[Self::Source],
			Tag::Process(_) => &[Self::Process],
			Tag::Signal(_) => &[Self::Signal],
			Tag::ProcessCompletion(_) => &[Self::ProcessCompletion],
			_ => {
				warn!("unhandled tag: {:?}", tag);
				&[]
			}
		}
	}
}

/// How a filter value is interpreted.
///
/// - `==` and `!=` match on the exact value as string equality (case-insensitively),
/// - `~=` and `~!` match using a [regex],
/// - `*=` and `*!` match using a glob, either via [globset] or [ignore]
/// - `:=` and `:!` match via exact string comparisons, but on any of the list of values separated
///   by `,`
/// - `=`, the "auto" operator, behaves as `*=` if the matcher is `Path`, and as `==` otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Op {
	/// The auto operator, `=`, resolves to `*=` or `==` depending on the matcher.
	Auto,

	/// The `==` operator, matches on exact string equality.
	Equal,

	/// The `!=` operator, matches on exact string inequality.
	NotEqual,

	/// The `~=` operator, matches on a regex.
	Regex,

	/// The `~!` operator, matches on a regex (matches are fails).
	NotRegex,

	/// The `*=` operator, matches on a glob.
	Glob,

	/// The `*!` operator, matches on a glob (matches are fails).
	NotGlob,

	/// The `:=` operator, matches (with string compares) on a set of values (belongs are passes).
	InSet,

	/// The `:!` operator, matches on a set of values (belongs are fails).
	NotInSet,
}

/// A filter value (pattern to match with).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Pattern {
	/// An exact string.
	Exact(String),

	/// A regex.
	Regex(Regex),

	/// A glob.
	///
	/// This is stored as a string as globs are compiled together rather than on a per-filter basis.
	Glob(String),

	/// A set of exact strings.
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
