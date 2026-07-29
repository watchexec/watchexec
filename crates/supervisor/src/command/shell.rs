use std::{borrow::Cow, ffi::OsStr, path::PathBuf};

/// Returns the default quoting value for the current platform.
///
/// On Windows this is false, and on Unix this is true.
/// This is to allow Unix and Windows to separately work as expected by default,
/// with logic elsewhere to allow opting in to/out of quoting.
const fn get_platform_default_quoting() -> bool {
	#[cfg(windows)]
	{
		false
	}

	#[cfg(unix)]
	{
		true
	}
}

/// How to call the shell used to run shelled programs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Shell {
	/// Path or name of the shell.
	pub prog: PathBuf,

	/// Additional options or arguments to pass to the shell.
	///
	/// These will be inserted before the `program_option` immediately preceding the program string.
	pub options: Vec<String>,

	/// The syntax of the option which precedes the program string.
	///
	/// For most shells, this is `-c`. On Windows, CMD.EXE prefers `/C`. If this is `None`, then no
	/// option is prepended; this may be useful for non-shell or non-standard shell programs.
	pub program_option: Option<Cow<'static, OsStr>>,

	/// Whether or not to quote the command symbols before passing them into the shell on Windows.
	///
	/// Command symbols will always be quoted on MacOS/Linux.
	pub quote: bool,
}

impl Shell {
	/// Shorthand for most shells, using the `-c` convention.
	pub fn with_quoting(name: impl Into<PathBuf>, quote: bool) -> Self {
		Self {
			prog: name.into(),
			options: Vec::new(),
			program_option: Some(Cow::Borrowed(OsStr::new("-c"))),
			quote,
		}
	}

	/// Shorthand for most shells, using the `-c` convention, with quoting set to the default
	/// expected for the current platform.
	pub fn new(name: impl Into<PathBuf>) -> Self {
		Self::with_quoting(name, get_platform_default_quoting())
	}

	#[cfg(windows)]
	#[must_use]
	/// Shorthand for the CMD.EXE shell.
	pub fn cmd_with_quoting(quote: bool) -> Self {
		Self {
			prog: "CMD.EXE".into(),
			options: Vec::new(),
			program_option: Some(Cow::Borrowed(OsStr::new("/C"))),
			quote,
		}
	}

	#[cfg(windows)]
	#[must_use]
	/// Shorthand for the CMD.EXE shell, with quoting set to the default for Windows (true).
	pub fn cmd() -> Self {
		Self::cmd_with_quoting(get_platform_default_quoting())
	}
}
