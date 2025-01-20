use std::{
	collections::BTreeSet,
	mem::take,
	path::{Path, PathBuf},
};

use clap::{Parser, ValueEnum, ValueHint};
use miette::{IntoDiagnostic, Result};
use tokio::{
	fs::File,
	io::{AsyncBufReadExt, BufReader},
};
use tracing::{debug, info};
use watchexec::{paths::PATH_SEPARATOR, WatchedPath};

use crate::filterer::parse::FilterProgram;

use super::{command::CommandArgs, OPTSET_FILTERING};

#[derive(Debug, Clone, Parser)]
pub struct FilteringArgs {
	#[doc(hidden)]
	#[arg(skip)]
	pub paths: Vec<WatchedPath>,

	/// Watch a specific file or directory
	///
	/// By default, Watchexec watches the current directory.
	///
	/// When watching a single file, it's often better to watch the containing directory instead,
	/// and filter on the filename. Some editors may replace the file with a new one when saving,
	/// and some platforms may not detect that or further changes.
	///
	/// Upon starting, Watchexec resolves a "project origin" from the watched paths. See the help
	/// for '--project-origin' for more information.
	///
	/// This option can be specified multiple times to watch multiple files or directories.
	///
	/// The special value '/dev/null', provided as the only path watched, will cause Watchexec to
	/// not watch any paths. Other event sources (like signals or key events) may still be used.
	#[arg(
		short = 'w',
		long = "watch",
		help_heading = OPTSET_FILTERING,
		value_hint = ValueHint::AnyPath,
		value_name = "PATH",
	)]
	pub recursive_paths: Vec<PathBuf>,

	/// Watch a specific directory, non-recursively
	///
	/// Unlike '-w', folders watched with this option are not recursed into.
	///
	/// This option can be specified multiple times to watch multiple directories non-recursively.
	#[arg(
		short = 'W',
		long = "watch-non-recursive",
		help_heading = OPTSET_FILTERING,
		value_hint = ValueHint::AnyPath,
		value_name = "PATH",
	)]
	pub non_recursive_paths: Vec<PathBuf>,

	/// Watch files and directories from a file
	///
	/// Each line in the file will be interpreted as if given to '-w'.
	///
	/// For more complex uses (like watching non-recursively), use the argfile capability: build a
	/// file containing command-line options and pass it to watchexec with `@path/to/argfile`.
	///
	/// The special value '-' will read from STDIN; this in incompatible with '--stdin-quit'.
	#[arg(
		short = 'F',
		long,
		help_heading = OPTSET_FILTERING,
		value_hint = ValueHint::AnyPath,
		value_name = "PATH",
	)]
	pub watch_file: Option<PathBuf>,

	/// Don't load gitignores
	///
	/// Among other VCS exclude files, like for Mercurial, Subversion, Bazaar, DARCS, Fossil. Note
	/// that Watchexec will detect which of these is in use, if any, and only load the relevant
	/// files. Both global (like '~/.gitignore') and local (like '.gitignore') files are considered.
	///
	/// This option is useful if you want to watch files that are ignored by Git.
	#[arg(
		long,
		help_heading = OPTSET_FILTERING,
	)]
	pub no_vcs_ignore: bool,

	/// Don't load project-local ignores
	///
	/// This disables loading of project-local ignore files, like '.gitignore' or '.ignore' in the
	/// watched project. This is contrasted with '--no-vcs-ignore', which disables loading of Git
	/// and other VCS ignore files, and with '--no-global-ignore', which disables loading of global
	/// or user ignore files, like '~/.gitignore' or '~/.config/watchexec/ignore'.
	///
	/// Supported project ignore files:
	///
	///   - Git: .gitignore at project root and child directories, .git/info/exclude, and the file pointed to by `core.excludesFile` in .git/config.
	///   - Mercurial: .hgignore at project root and child directories.
	///   - Bazaar: .bzrignore at project root.
	///   - Darcs: _darcs/prefs/boring
	///   - Fossil: .fossil-settings/ignore-glob
	///   - Ripgrep/Watchexec/generic: .ignore at project root and child directories.
	///
	/// VCS ignore files (Git, Mercurial, Bazaar, Darcs, Fossil) are only used if the corresponding
	/// VCS is discovered to be in use for the project/origin. For example, a .bzrignore in a Git
	/// repository will be discarded.
	#[arg(
		long,
		help_heading = OPTSET_FILTERING,
		verbatim_doc_comment,
	)]
	pub no_project_ignore: bool,

	/// Don't load global ignores
	///
	/// This disables loading of global or user ignore files, like '~/.gitignore',
	/// '~/.config/watchexec/ignore', or '%APPDATA%\Bazzar\2.0\ignore'. Contrast with
	/// '--no-vcs-ignore' and '--no-project-ignore'.
	///
	/// Supported global ignore files
	///
	///   - Git (if core.excludesFile is set): the file at that path
	///   - Git (otherwise): the first found of $XDG_CONFIG_HOME/git/ignore, %APPDATA%/.gitignore, %USERPROFILE%/.gitignore, $HOME/.config/git/ignore, $HOME/.gitignore.
	///   - Bazaar: the first found of %APPDATA%/Bazzar/2.0/ignore, $HOME/.bazaar/ignore.
	///   - Watchexec: the first found of $XDG_CONFIG_HOME/watchexec/ignore, %APPDATA%/watchexec/ignore, %USERPROFILE%/.watchexec/ignore, $HOME/.watchexec/ignore.
	///
	/// Like for project files, Git and Bazaar global files will only be used for the corresponding
	/// VCS as used in the project.
	#[arg(
		long,
		help_heading = OPTSET_FILTERING,
		verbatim_doc_comment,
	)]
	pub no_global_ignore: bool,

	/// Don't use internal default ignores
	///
	/// Watchexec has a set of default ignore patterns, such as editor swap files, `*.pyc`, `*.pyo`,
	/// `.DS_Store`, `.bzr`, `_darcs`, `.fossil-settings`, `.git`, `.hg`, `.pijul`, `.svn`, and
	/// Watchexec log files.
	#[arg(
		long,
		help_heading = OPTSET_FILTERING,
	)]
	pub no_default_ignore: bool,

	/// Don't discover ignore files at all
	///
	/// This is a shorthand for '--no-global-ignore', '--no-vcs-ignore', '--no-project-ignore', but
	/// even more efficient as it will skip all the ignore discovery mechanisms from the get go.
	///
	/// Note that default ignores are still loaded, see '--no-default-ignore'.
	#[arg(
		long,
		help_heading = OPTSET_FILTERING,
	)]
	pub no_discover_ignore: bool,

	/// Don't ignore anything at all
	///
	/// This is a shorthand for '--no-discover-ignore', '--no-default-ignore'.
	///
	/// Note that ignores explicitly loaded via other command line options, such as '--ignore' or
	/// '--ignore-file', will still be used.
	#[arg(
		long,
		help_heading = OPTSET_FILTERING,
	)]
	pub ignore_nothing: bool,

	/// Filename extensions to filter to
	///
	/// This is a quick filter to only emit events for files with the given extensions. Extensions
	/// can be given with or without the leading dot (e.g. 'js' or '.js'). Multiple extensions can
	/// be given by repeating the option or by separating them with commas.
	#[arg(
		long = "exts",
		short = 'e',
		help_heading = OPTSET_FILTERING,
		value_delimiter = ',',
		value_name = "EXTENSIONS",
	)]
	pub filter_extensions: Vec<String>,

	/// Filename patterns to filter to
	///
	/// Provide a glob-like filter pattern, and only events for files matching the pattern will be
	/// emitted. Multiple patterns can be given by repeating the option. Events that are not from
	/// files (e.g. signals, keyboard events) will pass through untouched.
	#[arg(
		long = "filter",
		short = 'f',
		help_heading = OPTSET_FILTERING,
		value_name = "PATTERN",
	)]
	pub filter_patterns: Vec<String>,

	/// Files to load filters from
	///
	/// Provide a path to a file containing filters, one per line. Empty lines and lines starting
	/// with '#' are ignored. Uses the same pattern format as the '--filter' option.
	///
	/// This can also be used via the $WATCHEXEC_FILTER_FILES environment variable.
	#[arg(
		long = "filter-file",
		help_heading = OPTSET_FILTERING,
		value_delimiter = PATH_SEPARATOR.chars().next().unwrap(),
		value_hint = ValueHint::FilePath,
		value_name = "PATH",
		env = "WATCHEXEC_FILTER_FILES",
		hide_env = true,
	)]
	pub filter_files: Vec<PathBuf>,

	/// Set the project origin
	///
	/// Watchexec will attempt to discover the project's "origin" (or "root") by searching for a
	/// variety of markers, like files or directory patterns. It does its best but sometimes gets it
	/// it wrong, and you can override that with this option.
	///
	/// The project origin is used to determine the path of certain ignore files, which VCS is being
	/// used, the meaning of a leading '/' in filtering patterns, and maybe more in the future.
	///
	/// When set, Watchexec will also not bother searching, which can be significantly faster.
	#[arg(
		long,
		help_heading = OPTSET_FILTERING,
		value_hint = ValueHint::DirPath,
		value_name = "DIRECTORY",
	)]
	pub project_origin: Option<PathBuf>,

	/// Filter programs.
	///
	/// Provide your own custom filter programs in jaq (similar to jq) syntax. Programs are given
	/// an event in the same format as described in '--emit-events-to' and must return a boolean.
	/// Invalid programs will make watchexec fail to start; use '-v' to see program runtime errors.
	///
	/// In addition to the jaq stdlib, watchexec adds some custom filter definitions:
	///
	///   - 'path | file_meta' returns file metadata or null if the file does not exist.
	///
	///   - 'path | file_size' returns the size of the file at path, or null if it does not exist.
	///
	///   - 'path | file_read(bytes)' returns a string with the first n bytes of the file at path.
	///     If the file is smaller than n bytes, the whole file is returned. There is no filter to
	///     read the whole file at once to encourage limiting the amount of data read and processed.
	///
	///   - 'string | hash', and 'path | file_hash' return the hash of the string or file at path.
	///     No guarantee is made about the algorithm used: treat it as an opaque value.
	///
	///   - 'any | kv_store(key)', 'kv_fetch(key)', and 'kv_clear' provide a simple key-value store.
	///     Data is kept in memory only, there is no persistence. Consistency is not guaranteed.
	///
	///   - 'any | printout', 'any | printerr', and 'any | log(level)' will print or log any given
	///     value to stdout, stderr, or the log (levels = error, warn, info, debug, trace), and
	///     pass the value through (so '[1] | log("debug") | .[]' will produce a '1' and log '[1]').
	///
	/// All filtering done with such programs, and especially those using kv or filesystem access,
	/// is much slower than the other filtering methods. If filtering is too slow, events will back
	/// up and stall watchexec. Take care when designing your filters.
	///
	/// If the argument to this option starts with an '@', the rest of the argument is taken to be
	/// the path to a file containing a jaq program.
	///
	/// Jaq programs are run in order, after all other filters, and short-circuit: if a filter (jaq
	/// or not) rejects an event, execution stops there, and no other filters are run. Additionally,
	/// they stop after outputting the first value, so you'll want to use 'any' or 'all' when
	/// iterating, otherwise only the first item will be processed, which can be quite confusing!
	///
	/// Find user-contributed programs or submit your own useful ones at
	/// <https://github.com/watchexec/watchexec/discussions/592>.
	///
	/// ## Examples:
	///
	/// Regexp ignore filter on paths:
	///
	///   'all(.tags[] | select(.kind == "path"); .absolute | test("[.]test[.]js$")) | not'
	///
	/// Pass any event that creates a file:
	///
	///   'any(.tags[] | select(.kind == "fs"); .simple == "create")'
	///
	/// Pass events that touch executable files:
	///
	///   'any(.tags[] | select(.kind == "path" && .filetype == "file"); .absolute | metadata | .executable)'
	///
	/// Ignore files that start with shebangs:
	///
	///   'any(.tags[] | select(.kind == "path" && .filetype == "file"); .absolute | read(2) == "#!") | not'
	#[arg(
		long = "filter-prog",
		short = 'j',
		help_heading = OPTSET_FILTERING,
		value_name = "EXPRESSION",
	)]
	pub filter_programs: Vec<String>,

	#[doc(hidden)]
	#[clap(skip)]
	pub filter_programs_parsed: Vec<FilterProgram>,

	/// Filename patterns to filter out
	///
	/// Provide a glob-like filter pattern, and events for files matching the pattern will be
	/// excluded. Multiple patterns can be given by repeating the option. Events that are not from
	/// files (e.g. signals, keyboard events) will pass through untouched.
	#[arg(
		long = "ignore",
		short = 'i',
		help_heading = OPTSET_FILTERING,
		value_name = "PATTERN",
	)]
	pub ignore_patterns: Vec<String>,

	/// Files to load ignores from
	///
	/// Provide a path to a file containing ignores, one per line. Empty lines and lines starting
	/// with '#' are ignored. Uses the same pattern format as the '--ignore' option.
	///
	/// This can also be used via the $WATCHEXEC_IGNORE_FILES environment variable.
	#[arg(
		long = "ignore-file",
		help_heading = OPTSET_FILTERING,
		value_delimiter = PATH_SEPARATOR.chars().next().unwrap(),
		value_hint = ValueHint::FilePath,
		value_name = "PATH",
		env = "WATCHEXEC_IGNORE_FILES",
		hide_env = true,
	)]
	pub ignore_files: Vec<PathBuf>,

	/// Filesystem events to filter to
	///
	/// This is a quick filter to only emit events for the given types of filesystem changes. Choose
	/// from 'access', 'create', 'remove', 'rename', 'modify', 'metadata'. Multiple types can be
	/// given by repeating the option or by separating them with commas. By default, this is all
	/// types except for 'access'.
	///
	/// This may apply filtering at the kernel level when possible, which can be more efficient, but
	/// may be more confusing when reading the logs.
	#[arg(
		long = "fs-events",
		help_heading = OPTSET_FILTERING,
		default_value = "create,remove,rename,modify,metadata",
		value_delimiter = ',',
		hide_default_value = true,
		value_name = "EVENTS",
	)]
	pub filter_fs_events: Vec<FsEvent>,

	/// Don't emit fs events for metadata changes
	///
	/// This is a shorthand for '--fs-events create,remove,rename,modify'. Using it alongside the
	/// '--fs-events' option is non-sensical and not allowed.
	#[arg(
		long = "no-meta",
		help_heading = OPTSET_FILTERING,
		conflicts_with = "filter_fs_events",
	)]
	pub filter_fs_meta: bool,
}

impl FilteringArgs {
	pub(crate) async fn normalise(&mut self, command: &CommandArgs) -> Result<()> {
		if self.ignore_nothing {
			self.no_global_ignore = true;
			self.no_vcs_ignore = true;
			self.no_project_ignore = true;
			self.no_default_ignore = true;
			self.no_discover_ignore = true;
		}

		if self.filter_fs_meta {
			self.filter_fs_events = vec![
				FsEvent::Create,
				FsEvent::Remove,
				FsEvent::Rename,
				FsEvent::Modify,
			];
		}

		if let Some(watch_file) = self.watch_file.as_ref() {
			if watch_file == Path::new("-") {
				let file = tokio::io::stdin();
				let mut lines = BufReader::new(file).lines();
				while let Ok(Some(line)) = lines.next_line().await {
					self.recursive_paths.push(line.into());
				}
			} else {
				let file = File::open(watch_file).await.into_diagnostic()?;
				let mut lines = BufReader::new(file).lines();
				while let Ok(Some(line)) = lines.next_line().await {
					self.recursive_paths.push(line.into());
				}
			};
		}

		let project_origin = if let Some(p) = take(&mut self.project_origin) {
			p
		} else {
			crate::dirs::project_origin(&self, command).await?
		};
		debug!(path=?project_origin, "resolved project origin");
		let project_origin = dunce::canonicalize(project_origin).into_diagnostic()?;
		info!(path=?project_origin, "effective project origin");
		self.project_origin = Some(project_origin.clone());

		self.paths = take(&mut self.recursive_paths)
			.into_iter()
			.map(|path| {
				{
					if path.is_absolute() {
						Ok(path)
					} else {
						dunce::canonicalize(project_origin.join(path)).into_diagnostic()
					}
				}
				.map(WatchedPath::recursive)
			})
			.chain(take(&mut self.non_recursive_paths).into_iter().map(|path| {
				{
					if path.is_absolute() {
						Ok(path)
					} else {
						dunce::canonicalize(project_origin.join(path)).into_diagnostic()
					}
				}
				.map(WatchedPath::non_recursive)
			}))
			.collect::<Result<BTreeSet<_>>>()?
			.into_iter()
			.collect();

		if self.paths.len() == 1
			&& self
				.paths
				.first()
				.map_or(false, |p| p.as_ref() == Path::new("/dev/null"))
		{
			info!("only path is /dev/null, not watching anything");
			self.paths = Vec::new();
		} else if self.paths.is_empty() {
			info!("no paths, using current directory");
			self.paths.push(command.workdir.as_deref().unwrap().into());
		}
		info!(paths=?self.paths, "effective watched paths");

		for (n, prog) in self.filter_programs.iter().enumerate() {
			if let Some(progpath) = prog.strip_prefix('@') {
				self.filter_programs_parsed
					.push(FilterProgram::new_jaq_from_file(progpath).await?);
			} else {
				self.filter_programs_parsed
					.push(FilterProgram::new_jaq_from_arg(n, prog.clone())?);
			}
		}

		debug_assert!(self.project_origin.is_some());
		Ok(())
	}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum FsEvent {
	Access,
	Create,
	Remove,
	Rename,
	Modify,
	Metadata,
}
