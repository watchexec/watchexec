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
	num::{NonZeroI32, NonZeroI64},
	path::{Path, PathBuf},
	process::ExitStatus,
};

use filekind::FileEventKind;

use crate::keyboard::Keyboard;
use crate::signal::{process::SubSignal, source::MainSignal};

/// Re-export of the Notify file event types.
pub mod filekind {
	pub use notify::event::{
		AccessKind, AccessMode, CreateKind, DataChange, EventKind as FileEventKind, MetadataKind,
		ModifyKind, RemoveKind, RenameMode,
	};
}

/// An event, as far as watchexec cares about.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Event {
	/// Structured, classified information which can be used to filter or classify the event.
	pub tags: Vec<Tag>,

	/// Arbitrary other information, cannot be used for filtering.
	pub metadata: HashMap<String, Vec<String>>,
}

/// Something which can be used to filter or qualify an event.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Tag {
	/// The event is about a path or file in the filesystem.
	Path {
		/// Path to the file or directory.
		path: PathBuf,

		/// Optional file type, if known.
		file_type: Option<FileType>,
	},

	/// Kind of a filesystem event (create, remove, modify, etc).
	FileEventKind(FileEventKind),

	/// The general source of the event.
	Source(Source),

	/// The event was caused by specific keyboard input
	Keyboard(Keyboard),

	/// The event was caused by a particular process.
	Process(u32),

	/// The event is about a signal being delivered to the main process.
	Signal(MainSignal),

	/// The event is about the subprocess ending.
	ProcessCompletion(Option<ProcessEnd>),
}

impl Tag {
	/// The name of the variant.
	pub const fn discriminant_name(&self) -> &'static str {
		match self {
			Self::Path { .. } => "Path",
			Self::FileEventKind(_) => "FileEventKind",
			Self::Source(_) => "Source",
			Self::Keyboard(_) => "Keyboard",
			Self::Process(_) => "Process",
			Self::Signal(_) => "Signal",
			Self::ProcessCompletion(_) => "ProcessCompletion",
		}
	}
}

/// The type of a file.
///
/// This is a simplification of the [`std::fs::FileType`] type, which is not constructable and may
/// differ on different platforms.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileType {
	/// A regular file.
	File,

	/// A directory.
	Dir,

	/// A symbolic link.
	Symlink,

	/// Something else.
	Other,
}

impl From<std::fs::FileType> for FileType {
	fn from(ft: std::fs::FileType) -> Self {
		if ft.is_file() {
			Self::File
		} else if ft.is_dir() {
			Self::Dir
		} else if ft.is_symlink() {
			Self::Symlink
		} else {
			Self::Other
		}
	}
}

impl fmt::Display for FileType {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::File => write!(f, "file"),
			Self::Dir => write!(f, "dir"),
			Self::Symlink => write!(f, "symlink"),
			Self::Other => write!(f, "other"),
		}
	}
}

/// The end status of a process.
///
/// This is a sort-of equivalent of the [`std::process::ExitStatus`] type, which is while
/// constructable, differs on various platforms. The native type is an integer that is interpreted
/// either through convention or via platform-dependent libc or kernel calls; our type is a more
/// structured representation for the purpose of being clearer and transportable.
///
/// On Unix, one can tell whether a process dumped core from the exit status; this is not replicated
/// in this structure; if that's desirable you can obtain it manually via `libc::WCOREDUMP` and the
/// `ExitSignal` variant.
///
/// On Unix and Windows, the exit status is a 32-bit integer; on Fuchsia it's a 64-bit integer. For
/// portability, we use `i64`. On all platforms, the "success" value is zero, so we special-case
/// that as a variant and use `NonZeroI*` to niche the other values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessEnd {
	/// The process ended successfully, with exit status = 0.
	Success,

	/// The process exited with a non-zero exit status.
	ExitError(NonZeroI64),

	/// The process exited due to a signal.
	ExitSignal(SubSignal),

	/// The process was stopped (but not terminated) (`libc::WIFSTOPPED`).
	ExitStop(NonZeroI32),

	/// The process suffered an unhandled exception or warning (typically Windows only).
	Exception(NonZeroI32),

	/// The process was continued (`libc::WIFCONTINUED`).
	Continued,
}

impl From<ExitStatus> for ProcessEnd {
	#[cfg(unix)]
	fn from(es: ExitStatus) -> Self {
		use std::os::unix::process::ExitStatusExt;

		match (es.code(), es.signal(), es.stopped_signal()) {
			(Some(_), Some(_), _) => {
				unreachable!("exitstatus cannot both be code and signal?!")
			}
			(Some(code), None, _) => match NonZeroI64::try_from(i64::from(code)) {
				Ok(code) => Self::ExitError(code),
				Err(_) => Self::Success,
			},
			(None, Some(_), Some(stopsig)) => {
				match NonZeroI32::try_from(stopsig) {
					Ok(signal) => Self::ExitStop(signal),
					Err(_) => Self::Success,
				}
			}
			#[cfg(not(target_os = "vxworks"))]
			(None, Some(_), _) if es.continued() => Self::Continued,
			(None, Some(signal), _) => Self::ExitSignal(signal.into()),
			(None, None, _) => Self::Success,
		}
	}

	#[cfg(windows)]
	fn from(es: ExitStatus) -> Self {
		match es.code().map(NonZeroI32::try_from) {
			None | Some(Err(_)) => Self::Success,
			Some(Ok(code)) if code.get() < 0 => Self::Exception(code),
			Some(Ok(code)) => Self::ExitError(code.into()),
		}
	}

	#[cfg(not(any(unix, windows)))]
	fn from(es: ExitStatus) -> Self {
		if es.success() {
			Self::Success
		} else {
			Self::ExitError(NonZeroI64::new(1).unwrap())
		}
	}
}

/// The general origin of the event.
///
/// This is set by the event source. Note that not all of these are currently used.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Source {
	/// Event comes from a file change.
	Filesystem,

	/// Event comes from a keyboard input.
	Keyboard,

	/// Event comes from a mouse click.
	Mouse,

	/// Event comes from the OS.
	Os,

	/// Event is time based.
	Time,

	/// Event is internal to Watchexec.
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

/// The priority of the event in the queue.
///
/// In the event queue, events are inserted with a priority, such that more important events are
/// delivered ahead of others. This is especially important when there is a large amount of events
/// generated and relatively slow filtering, as events can become noticeably delayed, and may give
/// the impression of stalling.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Priority {
	/// Low priority
	///
	/// Used for:
	/// - process completion events
	Low,

	/// Normal priority
	///
	/// Used for:
	/// - filesystem events
	Normal,

	/// High priority
	///
	/// Used for:
	/// - signals to main process, except Interrupt and Terminate
	High,

	/// Urgent events bypass filtering entirely.
	///
	/// Used for:
	/// - Interrupt and Terminate signals to main process
	Urgent,
}

impl Default for Priority {
	fn default() -> Self {
		Self::Normal
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
	pub fn paths(&self) -> impl Iterator<Item = (&Path, Option<&FileType>)> {
		self.tags.iter().filter_map(|p| match p {
			Tag::Path { path, file_type } => Some((path.as_path(), file_type.as_ref())),
			_ => None,
		})
	}

	/// Return all signals in the event's tags.
	pub fn signals(&self) -> impl Iterator<Item = MainSignal> + '_ {
		self.tags.iter().filter_map(|p| match p {
			Tag::Signal(s) => Some(*s),
			_ => None,
		})
	}

	/// Return all process completions in the event's tags.
	pub fn completions(&self) -> impl Iterator<Item = Option<ProcessEnd>> + '_ {
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
						write!(f, " filetype={ft}")?;
					}
				}
				Tag::FileEventKind(kind) => write!(f, " kind={kind:?}")?,
				Tag::Source(s) => write!(f, " source={s:?}")?,
				Tag::Keyboard(k) => write!(f, " keyboard={k:?}")?,
				Tag::Process(p) => write!(f, " process={p}")?,
				Tag::Signal(s) => write!(f, " signal={s:?}")?,
				Tag::ProcessCompletion(None) => write!(f, " command-completed")?,
				Tag::ProcessCompletion(Some(c)) => write!(f, " command-completed({c:?})")?,
			}
		}

		if !self.metadata.is_empty() {
			write!(f, " meta: {:?}", self.metadata)?;
		}

		Ok(())
	}
}
