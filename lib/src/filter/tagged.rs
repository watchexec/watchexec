//! A complex filterer that can match any event tag and supports different matching operators.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use dunce::canonicalize;
use globset::Glob;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::Match;
use tokio::fs::read_to_string;
use tracing::{debug, trace, trace_span, warn};
use unicase::UniCase;

use crate::error::RuntimeError;
use crate::event::{Event, FileType, ProcessEnd, Tag};
use crate::filter::tagged::error::TaggedFiltererError;
use crate::filter::Filterer;
use crate::ignore_files::IgnoreFile;
use crate::signal::process::SubSignal;
use crate::signal::source::MainSignal;

// to make filters
pub use regex::Regex;

pub mod error;
mod parse;
pub mod swaplock;

/// A filterer implementation that exposes the full capabilities of Watchexec.
///
/// **Note:** This filterer is experimental, and behaviour may change without semver notice. However,
/// types and its API are held to semver. This notice will eventually be removed when it stabilises.
///
/// Filters match against [event tags][Tag]; can be exact matches, glob matches, regex matches, or
/// set matches; can reverse the match (equal/not equal, etc); and can be negated.
///
/// [Filters][Filter] can be generated from your application and inserted directly, or they can be
/// parsed from a textual format:
///
/// ```text
/// [!]{Matcher}{Op}{Value}
/// ```
///
/// For example:
///
/// ```text
/// path==/foo/bar
/// path*=**/bar
/// path~=bar$
/// !kind=file
/// ```
///
/// There is a set of [operators][Op]:
/// - `==` and `!=`: exact match and exact not match (case insensitive)
/// - `~=` and `~!`: regex match and regex not match
/// - `*=` and `*!`: glob match and glob not match
/// - `:=` and `:!`: set match and set not match
///
/// Sets are a list of values separated by `,`.
///
/// In addition to the two-symbol operators, there is the `=` "auto" operator, which maps to the
/// most convenient operator for the given _matcher_. The current mapping is:
///
/// | Matcher                                         | Operator      |
/// |-------------------------------------------------|---------------|
/// | [Tag](Matcher::Tag)                             | `:=` (in set) |
/// | [Path](Matcher::Path)                           | `*=` (glob)   |
/// | [FileType](Matcher::FileType)                   | `:=` (in set) |
/// | [FileEventKind](Matcher::FileEventKind)         | `*=` (glob)   |
/// | [Source](Matcher::Source)                       | `:=` (in set) |
/// | [Process](Matcher::Process)                     | `:=` (in set) |
/// | [Signal](Matcher::Signal)                       | `:=` (in set) |
/// | [ProcessCompletion](Matcher::ProcessCompletion) | `*=` (glob) |
///
/// [Matchers][Matcher] correspond to Tags, but are not one-to-one: the `path` matcher operates on
/// the `path` part of the `Path` tag, and the `type` matcher operates on the `file_type`, for
/// example.
///
/// | Matcher                            | Syntax   | Tag                                          |
/// |------------------------------------|----------|----------------------------------------------|
/// | [Tag](Matcher::Tag)                | `tag`    | _the presence of a Tag on the event_         |
/// | [Path](Matcher::Path)              | `path`   | [Path](Tag::Path) (`path` field)             |
/// | [FileType](Matcher::FileType)      | `type`   | [Path](Tag::Path) (`file_type` field, when Some) |
/// | [FileEventKind](Matcher::FileEventKind) | `kind` or `fek` | [FileEventKind](Tag::FileEventKind) |
/// | [Source](Matcher::Source)          | `source` or `src`  | [Source](Tag::Source)              |
/// | [Process](Matcher::Process)        | `process` or `pid` | [Process](Tag::Process)            |
/// | [Signal](Matcher::Signal)          | `signal` | [Signal](Tag::Signal)                        |
/// | [ProcessCompletion](Matcher::ProcessCompletion) | `complete` or `exit` | [ProcessCompletion](Tag::ProcessCompletion) |
///
/// Filters are checked in order, grouped per tag and per matcher. Filter groups may be checked in
/// any order, but the filters in the groups are checked in add order. Path glob filters are always
/// checked first, for internal reasons.
///
/// The `negate` boolean field behaves specially: it is not operator negation, but rather the same
/// kind of behaviour that is applied to `!`-prefixed globs in gitignore files: if a negated filter
/// matches the event, the result of the event checking for that matcher is reverted to `true`, even
/// if a previous filter set it to `false`. Unmatched negated filters are ignored.
///
/// Glob syntax is as supported by the [ignore] crate for Paths, and by [globset] otherwise. (As of
/// writing, the ignore crate uses globset internally). Regex syntax is the default syntax of the
/// [regex] crate.
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

	/// Compiled matcher for Glob filters.
	glob_compiled: swaplock::SwapLock<Option<Gitignore>>,

	/// Compiled matcher for NotGlob filters.
	not_glob_compiled: swaplock::SwapLock<Option<Gitignore>>,
}

impl Filterer for TaggedFilterer {
	fn check_event(&self, event: &Event) -> Result<bool, RuntimeError> {
		self.check(event).map_err(|e| e.into())
	}
}

impl TaggedFilterer {
	fn check(&self, event: &Event) -> Result<bool, TaggedFiltererError> {
		let _span = trace_span!("filterer_check").entered();
		trace!(?event, "checking event");

		if self.filters.borrow().is_empty() {
			trace!("no filters, skipping entire check (pass)");
			return Ok(true);
		}

		trace!(tags=%event.tags.len(), "checking all tags on the event");
		for tag in &event.tags {
			let _span = trace_span!("check_tag", ?tag).entered();

			trace!("checking tag");
			for matcher in Matcher::from_tag(tag) {
				let _span = trace_span!("check_matcher", ?matcher).entered();

				let filters = self.filters.borrow().get(matcher).cloned();
				if let Some(tag_filters) = filters {
					if tag_filters.is_empty() {
						trace!("no filters for this matcher, skipping (pass)");
						continue;
					}

					trace!(filters=%tag_filters.len(), "found some filters for this matcher");

					let mut tag_match = true;

					if let (Matcher::Path, Tag::Path { path, file_type }) = (matcher, tag) {
						let is_dir = file_type.map_or(false, |ft| matches!(ft, FileType::Dir));

						let gc = self.glob_compiled.borrow();
						if let Some(igs) = gc.as_ref() {
							trace!("checking against compiled Glob filters");
							match if path.strip_prefix(&self.origin).is_ok() {
								trace!("checking against path or parents");
								igs.matched_path_or_any_parents(path, is_dir)
							} else {
								trace!("checking against path only");
								igs.matched(path, is_dir)
							} {
								Match::None => {
									trace!("no match (fail)");
									tag_match &= false;
								}
								Match::Ignore(glob) => {
									if glob.from().map_or(true, |f| path.strip_prefix(f).is_ok()) {
										trace!(?glob, "positive match (pass)");
										tag_match &= true;
									} else {
										trace!(?glob, "positive match, but not in scope (ignore)");
									}
								}
								Match::Whitelist(glob) => {
									trace!(?glob, "negative match (ignore)");
								}
							}
						}

						let ngc = self.not_glob_compiled.borrow();
						if let Some(ngs) = ngc.as_ref() {
							trace!("checking against compiled NotGlob filters");
							match if path.strip_prefix(&self.origin).is_ok() {
								trace!("checking against path or parents");
								ngs.matched_path_or_any_parents(path, is_dir)
							} else {
								trace!("checking against path only");
								ngs.matched(path, is_dir)
							} {
								Match::None => {
									trace!("no match (pass)");
									tag_match &= true;
								}
								Match::Ignore(glob) => {
									if glob.from().map_or(true, |f| path.strip_prefix(f).is_ok()) {
										trace!(?glob, "positive match (fail)");
										tag_match &= false;
									} else {
										trace!(?glob, "positive match, but not in scope (ignore)");
									}
								}
								Match::Whitelist(glob) => {
									trace!(?glob, "negative match (pass)");
									tag_match = true;
								}
							}
						}
					}

					// those are handled with the compiled ignore filters above
					let tag_filters = tag_filters
						.into_iter()
						.filter(|f| {
							!matches!(
								(tag, matcher, f),
								(
									Tag::Path { .. },
									Matcher::Path,
									Filter {
										on: Matcher::Path,
										op: Op::Glob | Op::NotGlob,
										pat: Pattern::Glob(_),
										..
									}
								)
							)
						})
						.collect::<Vec<_>>();
					if tag_filters.is_empty() && tag_match {
						trace!("no more filters for this matcher, skipping (pass)");
						continue;
					}

					trace!(filters=%tag_filters.len(), "got some filters to check still");

					for filter in &tag_filters {
						trace!(?filter, "checking filter againt tag");
						if let Some(app) = self.match_tag(filter, tag)? {
							if filter.negate {
								if app {
									trace!(prev=%tag_match, now=%true, "negate filter passes, passing this matcher");
									tag_match = true;
									break;
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
						trace!("matcher fails check, failing entire event");
						return Ok(false);
					}

					trace!("matcher passes check, continuing");
				} else {
					trace!("no filters for this matcher, skipping (pass)");
				}
			}
		}

		trace!("passing event");
		Ok(true)
	}

	/// Initialise a new tagged filterer with no filters.
	///
	/// This takes two paths: the project origin, and the current directory. The current directory
	/// is not obtained from the environment so you can customise it; generally you should use
	/// [`std::env::current_dir()`] though.
	///
	/// The origin is the directory the main project that is being watched is in. This is used to
	/// resolve absolute paths given in filters without an `in_path` field (e.g. all filters parsed
	/// from text).
	///
	/// The workdir is used to resolve relative paths given in filters without an `in_path` field.
	///
	/// So, if origin is `/path/to/project` and workdir is `/path/to/project/subtree`:
	/// - `path=foo.bar` is resolved to `/path/to/project/subtree/foo.bar`
	/// - `path=/foo.bar` is resolved to `/path/to/project/foo.bar`
	pub fn new(
		origin: impl Into<PathBuf>,
		workdir: impl Into<PathBuf>,
	) -> Result<Arc<Self>, TaggedFiltererError> {
		Ok(Arc::new(Self {
			origin: canonicalize(origin.into())?,
			workdir: canonicalize(workdir.into())?,
			filters: swaplock::SwapLock::new(HashMap::new()),
			glob_compiled: swaplock::SwapLock::new(None),
			not_glob_compiled: swaplock::SwapLock::new(None),
		}))
	}

	// filter ctx              event path                filter                 outcome
	// /foo/bar                /foo/bar/baz.txt          baz.txt                pass
	// /foo/bar                /foo/bar/baz.txt          /baz.txt               pass
	// /foo/bar                /foo/bar/baz.txt          /baz.*                 pass
	// /foo/bar                /foo/bar/baz.txt          /blah                  fail
	// /foo/quz                /foo/bar/baz.txt          /baz.*                 skip

	// Ok(Some(bool)) => the match was applied, bool is the result
	// Ok(None) => for some precondition, the match was not done (mismatched tag, out of context, …)
	fn match_tag(&self, filter: &Filter, tag: &Tag) -> Result<Option<bool>, TaggedFiltererError> {
		trace!(matcher=?filter.on, "matching filter to tag");
		match (tag, filter.on) {
			(tag, Matcher::Tag) => filter.matches(tag.discriminant_name()),
			(Tag::Path { path, .. }, Matcher::Path) => {
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

				if matches!(filter.op, Op::Glob | Op::NotGlob) {
					trace!("path glob match with match_tag is already handled");
					return Ok(None);
				} else {
					filter.matches(resolved.to_string_lossy())
				}
			}
			(
				Tag::Path {
					file_type: Some(ft),
					..
				},
				Matcher::FileType,
			) => filter.matches(ft.to_string()),
			(Tag::FileEventKind(kind), Matcher::FileEventKind) => {
				filter.matches(format!("{:?}", kind))
			}
			(Tag::Source(src), Matcher::Source) => filter.matches(src.to_string()),
			(Tag::Process(pid), Matcher::Process) => filter.matches(pid.to_string()),
			(Tag::Signal(sig), Matcher::Signal) => {
				let (text, int) = match sig {
					MainSignal::Hangup => ("HUP", 1),
					MainSignal::Interrupt => ("INT", 2),
					MainSignal::Quit => ("QUIT", 3),
					MainSignal::Terminate => ("TERM", 15),
					MainSignal::User1 => ("USR1", 10),
					MainSignal::User2 => ("USR2", 12),
				};

				Ok(filter.matches(text)?
					|| filter.matches(format!("SIG{}", text))?
					|| filter.matches(int.to_string())?)
			}
			(Tag::ProcessCompletion(ope), Matcher::ProcessCompletion) => match ope {
				None => filter.matches("_"),
				Some(ProcessEnd::Success) => filter.matches("success"),
				Some(ProcessEnd::ExitError(int)) => filter.matches(format!("error({})", int)),
				Some(ProcessEnd::ExitSignal(sig)) => {
					let (text, int) = match sig {
						SubSignal::Hangup | SubSignal::Custom(1) => ("HUP", 1),
						SubSignal::ForceStop | SubSignal::Custom(9) => ("KILL", 9),
						SubSignal::Interrupt | SubSignal::Custom(2) => ("INT", 2),
						SubSignal::Quit | SubSignal::Custom(3) => ("QUIT", 3),
						SubSignal::Terminate | SubSignal::Custom(15) => ("TERM", 15),
						SubSignal::User1 | SubSignal::Custom(10) => ("USR1", 10),
						SubSignal::User2 | SubSignal::Custom(12) => ("USR2", 12),
						SubSignal::Custom(n) => ("UNK", *n),
					};

					Ok(filter.matches(format!("signal({})", text))?
						|| filter.matches(format!("signal(SIG{})", text))?
						|| filter.matches(format!("signal({})", int))?)
				}
				Some(ProcessEnd::ExitStop(int)) => filter.matches(format!("stop({})", int)),
				Some(ProcessEnd::Exception(int)) => filter.matches(format!("exception({:X})", int)),
				Some(ProcessEnd::Continued) => filter.matches("continued"),
			},
			(_, _) => {
				trace!("no match for tag, skipping");
				return Ok(None);
			}
		}
		.map(Some)
	}

	/// Add some filters to the filterer.
	///
	/// This is async as it submits the new filters to the live filterer, which may be holding a
	/// read lock. It takes a slice of filters so it can efficiently add a large number of filters
	/// with a single write, without needing to acquire the lock repeatedly.
	///
	/// If filters with glob operations are added, the filterer's glob matchers are recompiled after
	/// the new filters are added, in this method.
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
		trace!(?filters, "canonicalised filters");
		// TODO: use miette's related and issue canonicalisation errors for all of them

		self.filters
			.change(|fs| {
				for filter in filters {
					fs.entry(filter.on).or_default().push(filter);
				}
			})
			.await
			.map_err(|err| TaggedFiltererError::FilterChange { action: "add", err })?;
		trace!("inserted filters into swaplock");

		if recompile_globs {
			self.recompile_globs(Op::Glob).await?;
		}

		if recompile_not_globs {
			self.recompile_globs(Op::NotGlob).await?;
		}

		Ok(())
	}

	async fn recompile_globs(&self, op_filter: Op) -> Result<(), TaggedFiltererError> {
		trace!(?op_filter, "recompiling globs");
		let target = match op_filter {
			Op::Glob => &self.glob_compiled,
			Op::NotGlob => &self.not_glob_compiled,
			_ => unreachable!("recompile_globs called with invalid op"),
		};

		let globs = {
			let filters = self.filters.borrow();
			if let Some(fs) = filters.get(&Matcher::Path) {
				trace!(?op_filter, "pulling filters from swaplock");
				// we want to hold the lock as little as possible, so we clone the filters
				fs.iter()
					.cloned()
					.filter(|f| f.op == op_filter)
					.collect::<Vec<_>>()
			} else {
				trace!(?op_filter, "no filters, erasing compiled glob");
				return target
					.replace(None)
					.await
					.map_err(TaggedFiltererError::GlobsetChange);
			}
		};

		let mut builder = GitignoreBuilder::new(&self.origin);
		for filter in globs {
			if let Pattern::Glob(mut glob) = filter.pat {
				if filter.negate {
					glob.insert(0, '!');
				}

				trace!(?op_filter, in_path=?filter.in_path, ?glob, "adding new glob line");
				builder
					.add_line(filter.in_path, &glob)
					.map_err(TaggedFiltererError::GlobParse)?;
			}
		}

		trace!(?op_filter, "finalising compiled glob");
		let compiled = builder.build().map_err(TaggedFiltererError::GlobParse)?;

		trace!(?op_filter, "swapping in new compiled glob");
		target
			.replace(Some(compiled))
			.await
			.map_err(TaggedFiltererError::GlobsetChange)
	}

	/// Reads a gitignore-style [`IgnoreFile`] and adds all of its contents to the filterer.
	///
	/// Empty lines and lines starting with `#` are ignored. The `applies_in` field of the
	/// [`IgnoreFile`] is used for the `in_path` field of each [`Filter`].
	///
	/// This method reads the entire file into memory.
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

	/// Clears all filters from the filterer.
	///
	/// This also recompiles the glob matchers, so essentially it resets the entire filterer state.
	pub async fn clear_filters(&self) -> Result<(), TaggedFiltererError> {
		debug!("removing all filters from filterer");
		self.filters
			.replace(Default::default())
			.await
			.map_err(|err| TaggedFiltererError::FilterChange {
				action: "clear all",
				err,
			})?;

		self.recompile_globs(Op::Glob).await?;
		self.recompile_globs(Op::NotGlob).await?;

		Ok(())
	}
}

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
			(op @ Op::Glob | op @ Op::NotGlob, Pattern::Glob(glob)) => {
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
	pub fn canonicalised(mut self) -> Result<Self, TaggedFiltererError> {
		if let Some(ctx) = self.in_path {
			self.in_path = Some(canonicalize(&ctx)?);
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
	///
	/// You should be extremely careful using this, as it's possible to make it impossible to quit
	/// Watchexec by e.g. not allowing signals to go through and thus ignoring Ctrl-C.
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
	/// string. On Windows, only these signal names is supported: `BREAK`, and `CTRL_C`. Matching is
	/// on both uppercase and lowercase forms.
	Signal,

	/// The exit status of a subprocess.
	///
	/// This is only present for events issued when the subprocess exits. The value is matched on
	/// both the exit code as an integer, and either `success` or `fail`, whichever succeeds.
	ProcessCompletion,
}

impl Matcher {
	fn from_tag(tag: &Tag) -> &'static [Self] {
		match tag {
			Tag::Path {
				file_type: None, ..
			} => &[Matcher::Path],
			Tag::Path { .. } => &[Matcher::Path, Matcher::FileType],
			Tag::FileEventKind(_) => &[Matcher::FileEventKind],
			Tag::Source(_) => &[Matcher::Source],
			Tag::Process(_) => &[Matcher::Process],
			Tag::Signal(_) => &[Matcher::Signal],
			Tag::ProcessCompletion(_) => &[Matcher::ProcessCompletion],
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
