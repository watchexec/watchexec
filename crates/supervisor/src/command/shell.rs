use std::{borrow::Cow, ffi::OsStr, path::PathBuf};

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
}

impl Shell {
	/// Shorthand for most shells, using the `-c` convention.
	pub fn new(name: impl Into<PathBuf>) -> Self {
		Self {
			prog: name.into(),
			options: Vec::new(),
			program_option: Some(Cow::Borrowed(OsStr::new("-c"))),
		}
	}

	#[cfg(windows)]
	#[must_use]
	/// Shorthand for the CMD.EXE shell.
	pub fn cmd() -> Self {
		Self {
			prog: "CMD.EXE".into(),
			options: Vec::new(),
			program_option: Some(Cow::Borrowed(OsStr::new("/C"))),
		}
	}
}
