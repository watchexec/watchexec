use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use dunce::canonicalize;
use ignore::{
	gitignore::{Gitignore, GitignoreBuilder},
	Match,
};
use ignore_files::IgnoreFile;
use tracing::{debug, trace, trace_span};

use watchexec::{
	error::RuntimeError,
	event::{Event, FileType, Priority, ProcessEnd, Tag},
	filter::Filterer,
	ignore::{IgnoreFilter, IgnoreFilterer},
	signal::{process::SubSignal, source::MainSignal},
};

use crate::{swaplock::SwapLock, Filter, Matcher, Op, Pattern, TaggedFiltererError};

/// A complex filterer that can match any event tag and supports different matching operators.
///
/// See the crate-level documentation for more information.
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
	filters: SwapLock<HashMap<Matcher, Vec<Filter>>>,

	/// Sub-filterer for ignore files.
	ignore_filterer: SwapLock<IgnoreFilterer>,

	/// Compiled matcher for Glob filters.
	glob_compiled: SwapLock<Option<Gitignore>>,

	/// Compiled matcher for NotGlob filters.
	not_glob_compiled: SwapLock<Option<Gitignore>>,
}

impl Filterer for TaggedFilterer {
	fn check_event(&self, event: &Event, priority: Priority) -> Result<bool, RuntimeError> {
		self.check(event, priority).map_err(|e| e.into())
	}
}

impl TaggedFilterer {
	fn check(&self, event: &Event, priority: Priority) -> Result<bool, TaggedFiltererError> {
		let _span = trace_span!("filterer_check").entered();
		trace!(?event, ?priority, "checking event");

		{
			trace!("checking priority");
			if let Some(filters) = self.filters.borrow().get(&Matcher::Priority).cloned() {
				trace!(filters=%filters.len(), "found some filters for priority");
				//
				let mut pri_match = true;
				for filter in &filters {
					let _span = trace_span!("checking filter against priority", ?filter).entered();
					let applies = filter.matches(match priority {
						Priority::Low => "low",
						Priority::Normal => "normal",
						Priority::High => "high",
						Priority::Urgent => unreachable!("urgent by-passes filtering"),
					})?;
					if filter.negate {
						if applies {
							trace!(prev=%pri_match, now=%true, "negate filter passes, passing this priority");
							pri_match = true;
							break;
						} else {
							trace!(prev=%pri_match, now=%pri_match, "negate filter fails, ignoring");
						}
					} else {
						trace!(prev=%pri_match, this=%applies, now=%(pri_match&applies), "filter applies to priority");
						pri_match &= applies;
					}
				}

				if !pri_match {
					trace!("priority fails check, failing entire event");
					return Ok(false);
				}
			} else {
				trace!("no filters for priority, skipping (pass)");
			}
		}

		{
			trace!("checking internal ignore filterer");
			let igf = self.ignore_filterer.borrow();
			if !igf
				.check_event(event, priority)
				.expect("IgnoreFilterer never errors")
			{
				trace!("internal ignore filterer matched (fail)");
				return Ok(false);
			}
		}

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

						{
							let gc = self.glob_compiled.borrow();
							if let Some(igs) = gc.as_ref() {
								let _span =
									trace_span!("checking_compiled_filters", compiled=%"Glob")
										.entered();
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
										if glob
											.from()
											.map_or(true, |f| path.strip_prefix(f).is_ok())
										{
											trace!(?glob, "positive match (pass)");
											tag_match &= true;
										} else {
											trace!(
												?glob,
												"positive match, but not in scope (ignore)"
											);
										}
									}
									Match::Whitelist(glob) => {
										trace!(?glob, "negative match (ignore)");
									}
								}
							}
						}

						{
							let ngc = self.not_glob_compiled.borrow();
							if let Some(ngs) = ngc.as_ref() {
								let _span =
									trace_span!("checking_compiled_filters", compiled=%"NotGlob")
										.entered();
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
										if glob
											.from()
											.map_or(true, |f| path.strip_prefix(f).is_ok())
										{
											trace!(?glob, "positive match (fail)");
											tag_match &= false;
										} else {
											trace!(
												?glob,
												"positive match, but not in scope (ignore)"
											);
										}
									}
									Match::Whitelist(glob) => {
										trace!(?glob, "negative match (pass)");
										tag_match = true;
									}
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
						let _span = trace_span!("checking filter against tag", ?filter).entered();
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
	/// from text), and for ignore file based filtering.
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
		let origin = canonicalize(origin.into()).map_err(|err| TaggedFiltererError::IoError {
			about: "canonicalise origin on new tagged filterer",
			err,
		})?;
		Ok(Arc::new(Self {
			filters: SwapLock::new(HashMap::new()),
			ignore_filterer: SwapLock::new(IgnoreFilterer(IgnoreFilter::empty(&origin))),
			glob_compiled: SwapLock::new(None),
			not_glob_compiled: SwapLock::new(None),
			workdir: canonicalize(workdir.into()).map_err(|err| TaggedFiltererError::IoError {
				about: "canonicalise workdir on new tagged filterer",
				err,
			})?,
			origin,
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
	/// the new filters are added, in this method. This should not be used for inserting an
	/// [`IgnoreFile`]: use [`add_ignore_file()`](Self::add_ignore_file) instead.
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

	/// Reads a gitignore-style [`IgnoreFile`] and adds it to the filterer.
	pub async fn add_ignore_file(&self, file: &IgnoreFile) -> Result<(), TaggedFiltererError> {
		let mut new = { self.ignore_filterer.borrow().clone() };

		new.0
			.add_file(file)
			.await
			.map_err(TaggedFiltererError::Ignore)?;
		self.ignore_filterer
			.replace(new)
			.await
			.map_err(TaggedFiltererError::IgnoreSwap)?;
		Ok(())
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
