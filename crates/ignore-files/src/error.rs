use std::path::PathBuf;

use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
#[non_exhaustive]
pub enum Error {
	/// Error received when an [`IgnoreFile`] cannot be read.
	///
	/// [`IgnoreFile`]: crate::IgnoreFile
	#[error("cannot read ignore '{file}': {err}")]
	#[diagnostic(code(ignore_file::read))]
	Read {
		/// The path to the erroring ignore file.
		file: PathBuf,

		/// The underlying error.
		#[source]
		err: std::io::Error,
	},

	/// Error received when parsing a glob fails.
	#[error("cannot parse glob from ignore '{file:?}': {err}")]
	#[diagnostic(code(ignore_file::glob))]
	Glob {
		/// The path to the erroring ignore file.
		file: Option<PathBuf>,

		/// The underlying error.
		#[source]
		err: ignore::Error,
		// TODO: extract glob error into diagnostic
	},

	/// Multiple related [`Error`]s.
	#[error("multiple: {0:?}")]
	#[diagnostic(code(ignore_file::set))]
	Multi(#[related] Vec<Error>),
}
