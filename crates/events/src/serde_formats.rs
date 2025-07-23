use std::{
	collections::BTreeMap,
	num::{NonZeroI32, NonZeroI64},
	path::PathBuf,
};

use serde::{Deserialize, Serialize};
use watchexec_signals::Signal;

use crate::{
	fs::filekind::{
		AccessKind, AccessMode, CreateKind, DataChange, FileEventKind as EventKind, MetadataKind,
		ModifyKind, RemoveKind, RenameMode,
	},
	Event, FileType, Keyboard, ProcessEnd, Source, Tag,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SerdeTag {
	kind: TagKind,

	// path
	#[serde(default, skip_serializing_if = "Option::is_none")]
	absolute: Option<PathBuf>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	filetype: Option<FileType>,

	// fs
	#[serde(default, skip_serializing_if = "Option::is_none")]
	simple: Option<FsEventKind>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	full: Option<String>,

	// source
	#[serde(default, skip_serializing_if = "Option::is_none")]
	source: Option<Source>,

	// keyboard
	#[serde(default, skip_serializing_if = "Option::is_none")]
	keycode: Option<Keyboard>,

	// process
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pid: Option<u32>,

	// signal
	#[serde(default, skip_serializing_if = "Option::is_none")]
	signal: Option<Signal>,

	// completion
	#[serde(default, skip_serializing_if = "Option::is_none")]
	disposition: Option<ProcessDisposition>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	code: Option<i64>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TagKind {
	#[default]
	None,
	Path,
	Fs,
	Source,
	Keyboard,
	Process,
	Signal,
	Completion,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProcessDisposition {
	Unknown,
	Success,
	Error,
	Signal,
	Stop,
	Exception,
	Continued,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FsEventKind {
	Access,
	Create,
	Modify,
	Remove,
	Other,
}

impl From<EventKind> for FsEventKind {
	fn from(value: EventKind) -> Self {
		match value {
			EventKind::Access(_) => Self::Access,
			EventKind::Create(_) => Self::Create,
			EventKind::Modify(_) => Self::Modify,
			EventKind::Remove(_) => Self::Remove,
			EventKind::Any | EventKind::Other => Self::Other,
		}
	}
}

impl From<Tag> for SerdeTag {
	fn from(value: Tag) -> Self {
		match value {
			Tag::Path { path, file_type } => Self {
				kind: TagKind::Path,
				absolute: Some(path),
				filetype: file_type,
				..Default::default()
			},
			Tag::FileEventKind(fek) => Self {
				kind: TagKind::Fs,
				full: Some(format!("{fek:?}")),
				simple: Some(fek.into()),
				..Default::default()
			},
			Tag::Source(source) => Self {
				kind: TagKind::Source,
				source: Some(source),
				..Default::default()
			},
			Tag::Keyboard(keycode) => Self {
				kind: TagKind::Keyboard,
				keycode: Some(keycode),
				..Default::default()
			},
			Tag::Process(pid) => Self {
				kind: TagKind::Process,
				pid: Some(pid),
				..Default::default()
			},
			Tag::Signal(signal) => Self {
				kind: TagKind::Signal,
				signal: Some(signal),
				..Default::default()
			},
			Tag::ProcessCompletion(None) => Self {
				kind: TagKind::Completion,
				disposition: Some(ProcessDisposition::Unknown),
				..Default::default()
			},
			Tag::ProcessCompletion(Some(end)) => Self {
				kind: TagKind::Completion,
				code: match &end {
					ProcessEnd::Success | ProcessEnd::Continued | ProcessEnd::ExitSignal(_) => None,
					ProcessEnd::ExitError(err) => Some(err.get()),
					ProcessEnd::ExitStop(code) => Some(code.get().into()),
					ProcessEnd::Exception(exc) => Some(exc.get().into()),
				},
				signal: if let ProcessEnd::ExitSignal(sig) = &end {
					Some(*sig)
				} else {
					None
				},
				disposition: Some(match end {
					ProcessEnd::Success => ProcessDisposition::Success,
					ProcessEnd::ExitError(_) => ProcessDisposition::Error,
					ProcessEnd::ExitSignal(_) => ProcessDisposition::Signal,
					ProcessEnd::ExitStop(_) => ProcessDisposition::Stop,
					ProcessEnd::Exception(_) => ProcessDisposition::Exception,
					ProcessEnd::Continued => ProcessDisposition::Continued,
				}),
				..Default::default()
			},
			Tag::Unknown => Self::default(),
		}
	}
}

#[allow(
	clippy::fallible_impl_from,
	reason = "this triggers due to the unwraps, which are checked by branches"
)]
#[allow(
	clippy::too_many_lines,
	reason = "clearer as a single match tree than broken up"
)]
impl From<SerdeTag> for Tag {
	fn from(value: SerdeTag) -> Self {
		match value {
			SerdeTag {
				kind: TagKind::Path,
				absolute: Some(path),
				filetype,
				..
			} => Self::Path {
				path,
				file_type: filetype,
			},
			SerdeTag {
				kind: TagKind::Fs,
				full: Some(full),
				..
			} => Self::FileEventKind(match full.as_str() {
				"Any" => EventKind::Any,
				"Access(Any)" => EventKind::Access(AccessKind::Any),
				"Access(Read)" => EventKind::Access(AccessKind::Read),
				"Access(Open(Any))" => EventKind::Access(AccessKind::Open(AccessMode::Any)),
				"Access(Open(Execute))" => EventKind::Access(AccessKind::Open(AccessMode::Execute)),
				"Access(Open(Read))" => EventKind::Access(AccessKind::Open(AccessMode::Read)),
				"Access(Open(Write))" => EventKind::Access(AccessKind::Open(AccessMode::Write)),
				"Access(Open(Other))" => EventKind::Access(AccessKind::Open(AccessMode::Other)),
				"Access(Close(Any))" => EventKind::Access(AccessKind::Close(AccessMode::Any)),
				"Access(Close(Execute))" => {
					EventKind::Access(AccessKind::Close(AccessMode::Execute))
				}
				"Access(Close(Read))" => EventKind::Access(AccessKind::Close(AccessMode::Read)),
				"Access(Close(Write))" => EventKind::Access(AccessKind::Close(AccessMode::Write)),
				"Access(Close(Other))" => EventKind::Access(AccessKind::Close(AccessMode::Other)),
				"Access(Other)" => EventKind::Access(AccessKind::Other),
				"Create(Any)" => EventKind::Create(CreateKind::Any),
				"Create(File)" => EventKind::Create(CreateKind::File),
				"Create(Folder)" => EventKind::Create(CreateKind::Folder),
				"Create(Other)" => EventKind::Create(CreateKind::Other),
				"Modify(Any)" => EventKind::Modify(ModifyKind::Any),
				"Modify(Data(Any))" => EventKind::Modify(ModifyKind::Data(DataChange::Any)),
				"Modify(Data(Size))" => EventKind::Modify(ModifyKind::Data(DataChange::Size)),
				"Modify(Data(Content))" => EventKind::Modify(ModifyKind::Data(DataChange::Content)),
				"Modify(Data(Other))" => EventKind::Modify(ModifyKind::Data(DataChange::Other)),
				"Modify(Metadata(Any))" => {
					EventKind::Modify(ModifyKind::Metadata(MetadataKind::Any))
				}
				"Modify(Metadata(AccessTime))" => {
					EventKind::Modify(ModifyKind::Metadata(MetadataKind::AccessTime))
				}
				"Modify(Metadata(WriteTime))" => {
					EventKind::Modify(ModifyKind::Metadata(MetadataKind::WriteTime))
				}
				"Modify(Metadata(Permissions))" => {
					EventKind::Modify(ModifyKind::Metadata(MetadataKind::Permissions))
				}
				"Modify(Metadata(Ownership))" => {
					EventKind::Modify(ModifyKind::Metadata(MetadataKind::Ownership))
				}
				"Modify(Metadata(Extended))" => {
					EventKind::Modify(ModifyKind::Metadata(MetadataKind::Extended))
				}
				"Modify(Metadata(Other))" => {
					EventKind::Modify(ModifyKind::Metadata(MetadataKind::Other))
				}
				"Modify(Name(Any))" => EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
				"Modify(Name(To))" => EventKind::Modify(ModifyKind::Name(RenameMode::To)),
				"Modify(Name(From))" => EventKind::Modify(ModifyKind::Name(RenameMode::From)),
				"Modify(Name(Both))" => EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
				"Modify(Name(Other))" => EventKind::Modify(ModifyKind::Name(RenameMode::Other)),
				"Modify(Other)" => EventKind::Modify(ModifyKind::Other),
				"Remove(Any)" => EventKind::Remove(RemoveKind::Any),
				"Remove(File)" => EventKind::Remove(RemoveKind::File),
				"Remove(Folder)" => EventKind::Remove(RemoveKind::Folder),
				"Remove(Other)" => EventKind::Remove(RemoveKind::Other),
				_ => EventKind::Other, // and literal "Other"
			}),
			SerdeTag {
				kind: TagKind::Fs,
				simple: Some(simple),
				..
			} => Self::FileEventKind(match simple {
				FsEventKind::Access => EventKind::Access(AccessKind::Any),
				FsEventKind::Create => EventKind::Create(CreateKind::Any),
				FsEventKind::Modify => EventKind::Modify(ModifyKind::Any),
				FsEventKind::Remove => EventKind::Remove(RemoveKind::Any),
				FsEventKind::Other => EventKind::Other,
			}),
			SerdeTag {
				kind: TagKind::Source,
				source: Some(source),
				..
			} => Self::Source(source),
			SerdeTag {
				kind: TagKind::Keyboard,
				keycode: Some(keycode),
				..
			} => Self::Keyboard(keycode),
			SerdeTag {
				kind: TagKind::Process,
				pid: Some(pid),
				..
			} => Self::Process(pid),
			SerdeTag {
				kind: TagKind::Signal,
				signal: Some(sig),
				..
			} => Self::Signal(sig),
			SerdeTag {
				kind: TagKind::Completion,
				disposition: None | Some(ProcessDisposition::Unknown),
				..
			} => Self::ProcessCompletion(None),
			SerdeTag {
				kind: TagKind::Completion,
				disposition: Some(ProcessDisposition::Success),
				..
			} => Self::ProcessCompletion(Some(ProcessEnd::Success)),
			SerdeTag {
				kind: TagKind::Completion,
				disposition: Some(ProcessDisposition::Continued),
				..
			} => Self::ProcessCompletion(Some(ProcessEnd::Continued)),
			SerdeTag {
				kind: TagKind::Completion,
				disposition: Some(ProcessDisposition::Signal),
				signal: Some(sig),
				..
			} => Self::ProcessCompletion(Some(ProcessEnd::ExitSignal(sig))),
			SerdeTag {
				kind: TagKind::Completion,
				disposition: Some(ProcessDisposition::Error),
				code: Some(err),
				..
			} if err != 0 => Self::ProcessCompletion(Some(ProcessEnd::ExitError(unsafe {
				NonZeroI64::new_unchecked(err)
			}))),
			SerdeTag {
				kind: TagKind::Completion,
				disposition: Some(ProcessDisposition::Stop),
				code: Some(code),
				..
			} if code != 0 && i32::try_from(code).is_ok() => {
				Self::ProcessCompletion(Some(ProcessEnd::ExitStop(unsafe {
					// SAFETY&UNWRAP: checked above
					NonZeroI32::new_unchecked(code.try_into().unwrap())
				})))
			}
			SerdeTag {
				kind: TagKind::Completion,
				disposition: Some(ProcessDisposition::Exception),
				code: Some(exc),
				..
			} if exc != 0 && i32::try_from(exc).is_ok() => {
				Self::ProcessCompletion(Some(ProcessEnd::Exception(unsafe {
					// SAFETY&UNWRAP: checked above
					NonZeroI32::new_unchecked(exc.try_into().unwrap())
				})))
			}
			_ => Self::Unknown,
		}
	}
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SerdeEvent {
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	tags: Vec<Tag>,

	// for a consistent serialization order
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	metadata: BTreeMap<String, Vec<String>>,
}

impl From<Event> for SerdeEvent {
	fn from(Event { tags, metadata }: Event) -> Self {
		Self {
			tags,
			metadata: metadata.into_iter().collect(),
		}
	}
}

impl From<SerdeEvent> for Event {
	fn from(SerdeEvent { tags, metadata }: SerdeEvent) -> Self {
		Self {
			tags,
			metadata: metadata.into_iter().collect(),
		}
	}
}
