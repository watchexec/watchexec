use std::{
	collections::{hash_map::DefaultHasher, HashMap},
	fmt,
	hash::{self, Hash, Hasher},
	path::{Path, PathBuf},
};

use watchexec_signals::Signal;

#[cfg(feature = "serde")]
use crate::serde_formats::{SerdeEvent, SerdeTag};

use crate::{filekind::FileEventKind, FileType, Keyboard, ProcessEnd};

/// An event, as far as watchexec cares about.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(from = "SerdeEvent", into = "SerdeEvent"))]
pub struct Event {
	/// Structured, classified information which can be used to filter or classify the event.
	pub tags: Vec<Tag>,

	/// Arbitrary other information, cannot be used for filtering.
	pub metadata: HashMap<String, Vec<String>>,

	pub id: Option<EventId>,
}

impl hash::Hash for Event {
	fn hash<H: hash::Hasher>(&self, state: &mut H) {
		self.tags.hash(state);
		self.metadata.iter().for_each(|(k, v)| {
			k.hash(state);
			v.hash(state);
		})
	}
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct EventId(usize);

/// Something which can be used to filter or qualify an event.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(from = "SerdeTag", into = "SerdeTag"))]
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

	/// The event is about a keyboard input.
	Keyboard(Keyboard),

	/// The event was caused by a particular process.
	Process(u32),

	/// The event is about a signal being delivered to the main process.
	Signal(Signal),

	/// The event is about the subprocess ending.
	ProcessCompletion(Option<ProcessEnd>),

	#[cfg(feature = "serde")]
	/// The event is unknown (or not yet implemented).
	Unknown,
}

impl hash::Hash for Tag {
	fn hash<H: hash::Hasher>(&self, state: &mut H) {
		match self {
			Self::Path { path, file_type: _ } => path.hash(state),
			Self::FileEventKind(x) => x.hash(state),
			Self::Source(x) => x.hash(state),
			Self::Keyboard(x) => x.hash(state),
			Self::Process(x) => x.hash(state),
			Self::Signal(x) => x.hash(state),
			Self::ProcessCompletion(x) => x.hash(state),
			Self::Unknown => self.discriminant_name().hash(state),
		}
	}
}

impl Tag {
	/// The name of the variant.
	#[must_use]
	pub const fn discriminant_name(&self) -> &'static str {
		match self {
			Self::Path { .. } => "Path",
			Self::FileEventKind(_) => "FileEventKind",
			Self::Source(_) => "Source",
			Self::Keyboard(_) => "Keyboard",
			Self::Process(_) => "Process",
			Self::Signal(_) => "Signal",
			Self::ProcessCompletion(_) => "ProcessCompletion",
			#[cfg(feature = "serde")]
			Self::Unknown => "Unknown",
		}
	}
}

/// The general origin of the event.
///
/// This is set by the event source. Note that not all of these are currently used.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
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
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
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
	#[must_use]
	pub fn is_internal(&self) -> bool {
		self.tags
			.iter()
			.any(|tag| matches!(tag, Tag::Source(Source::Internal)))
	}

	/// Returns true if the event has no tags.
	#[must_use]
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
	pub fn signals(&self) -> impl Iterator<Item = Signal> + '_ {
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

	pub fn id(&mut self) -> EventId {
		*self.id.get_or_insert({
			let mut hasher = DefaultHasher::new();
			self.hash(&mut hasher);
			EventId(hasher.finish() as usize)
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
				#[cfg(feature = "serde")]
				Tag::Unknown => write!(f, " unknown")?,
			}
		}

		if !self.metadata.is_empty() {
			write!(f, " meta: {:?}", self.metadata)?;
		}

		Ok(())
	}
}
