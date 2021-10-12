//! Synthetic event type, derived from inputs, triggers actions.
//!
//! Fundamentally, events in watchexec have three purposes:
//!
//! 1. To trigger the launch, restart, or other interruption of a process;
//! 2. To be filtered upon according to whatever set of criteria is desired;
//! 3. To carry information about what caused the event, which may be provided to the process.

use std::{
	collections::HashMap,
	fmt,
	fs::FileType,
	path::{Path, PathBuf},
	process::ExitStatus,
};

use notify::EventKind;

use crate::signal::Signal;

/// An event, as far as watchexec cares about.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Event {
	pub tags: Vec<Tag>,
	pub metadata: HashMap<String, Vec<String>>,
}

/// Something which can be used to filter or qualify an event.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Tag {
	Path {
		path: PathBuf,
		file_type: Option<FileType>,
	},
	FileEventKind(EventKind),
	Source(Source),
	Process(u32),
	Signal(Signal),
	ProcessCompletion(Option<ExitStatus>),
}

impl Tag {
	pub const fn discriminant_name(&self) -> &'static str {
		match self {
			Tag::Path { .. } => "Path",
			Tag::FileEventKind(_) => "FileEventKind",
			Tag::Source(_) => "Source",
			Tag::Process(_) => "Process",
			Tag::Signal(_) => "Signal",
			Tag::ProcessCompletion(_) => "ProcessCompletion",
		}
	}
}

/// The general origin of the event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Source {
	Filesystem,
	Keyboard,
	Mouse,
	Os,
	Time,
	Internal,
}

impl fmt::Display for Source {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f,
			"{}",
			match self {
				Self::Filesystem => "filesystem",
				Self::Keyboard => "keyboard",
				Self::Mouse => "mouse",
				Self::Os => "os",
				Self::Time => "time",
				Self::Internal => "internal",
			}
		)
	}
}

impl Event {
	/// Returns true if the event has an Internal source tag.
	pub fn is_internal(&self) -> bool {
		self.tags
			.iter()
			.any(|tag| matches!(tag, Tag::Source(Source::Internal)))
	}

	/// Returns true if the event has no tags.
	pub fn is_empty(&self) -> bool {
		self.tags.is_empty()
	}

	/// Return all paths in the event's tags.
	pub fn paths(&self) -> impl Iterator<Item = &Path> {
		self.tags.iter().filter_map(|p| match p {
			Tag::Path { path, .. } => Some(path.as_path()),
			_ => None,
		})
	}

	/// Return all signals in the event's tags.
	pub fn signals(&self) -> impl Iterator<Item = Signal> + '_ {
		self.tags.iter().filter_map(|p| match p {
			Tag::Signal(s) => Some(*s),
			_ => None,
		})
	}

	/// Return all process completions in the event's tags.
	pub fn completions(&self) -> impl Iterator<Item = Option<ExitStatus>> + '_ {
		self.tags.iter().filter_map(|p| match p {
			Tag::ProcessCompletion(s) => Some(*s),
			_ => None,
		})
	}
}

impl fmt::Display for Event {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "Event")?;
		for p in &self.tags {
			match p {
				Tag::Path { path, file_type } => {
					write!(f, " path={}", path.display())?;
					if let Some(ft) = file_type {
						write!(
							f,
							" filetype={}",
							if ft.is_file() {
								"file"
							} else if ft.is_dir() {
								"dir"
							} else if ft.is_symlink() {
								"symlink"
							} else {
								"special"
							}
						)?;
					}
				}
				Tag::FileEventKind(kind) => write!(f, " kind={:?}", kind)?,
				Tag::Source(s) => write!(f, " source={:?}", s)?,
				Tag::Process(p) => write!(f, " process={}", p)?,
				Tag::Signal(s) => write!(f, " signal={:?}", s)?,
				Tag::ProcessCompletion(None) => write!(f, " command-completed")?,
				Tag::ProcessCompletion(Some(c)) => write!(f, " command-completed({})", c)?,
			}
		}

		if !self.metadata.is_empty() {
			write!(f, " meta: {:?}", self.metadata)?;
		}

		Ok(())
	}
}
