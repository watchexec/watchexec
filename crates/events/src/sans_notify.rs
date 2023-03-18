// This file is dual-licensed under the Artistic License 2.0 as per the
// LICENSE.ARTISTIC file, and the Creative Commons Zero 1.0 license.
//
// Taken verbatim from the `notify` crate, with the Event types removed.

use std::hash::Hash;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// An event describing open or close operations on files.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum AccessMode {
	/// The catch-all case, to be used when the specific kind of event is unknown.
	Any,

	/// An event emitted when the file is executed, or the folder opened.
	Execute,

	/// An event emitted when the file is opened for reading.
	Read,

	/// An event emitted when the file is opened for writing.
	Write,

	/// An event which specific kind is known but cannot be represented otherwise.
	Other,
}

/// An event describing non-mutating access operations on files.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", content = "mode"))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum AccessKind {
	/// The catch-all case, to be used when the specific kind of event is unknown.
	Any,

	/// An event emitted when the file is read.
	Read,

	/// An event emitted when the file, or a handle to the file, is opened.
	Open(AccessMode),

	/// An event emitted when the file, or a handle to the file, is closed.
	Close(AccessMode),

	/// An event which specific kind is known but cannot be represented otherwise.
	Other,
}

/// An event describing creation operations on files.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind"))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum CreateKind {
	/// The catch-all case, to be used when the specific kind of event is unknown.
	Any,

	/// An event which results in the creation of a file.
	File,

	/// An event which results in the creation of a folder.
	Folder,

	/// An event which specific kind is known but cannot be represented otherwise.
	Other,
}

/// An event emitted when the data content of a file is changed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum DataChange {
	/// The catch-all case, to be used when the specific kind of event is unknown.
	Any,

	/// An event emitted when the size of the data is changed.
	Size,

	/// An event emitted when the content of the data is changed.
	Content,

	/// An event which specific kind is known but cannot be represented otherwise.
	Other,
}

/// An event emitted when the metadata of a file or folder is changed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum MetadataKind {
	/// The catch-all case, to be used when the specific kind of event is unknown.
	Any,

	/// An event emitted when the access time of the file or folder is changed.
	AccessTime,

	/// An event emitted when the write or modify time of the file or folder is changed.
	WriteTime,

	/// An event emitted when the permissions of the file or folder are changed.
	Permissions,

	/// An event emitted when the ownership of the file or folder is changed.
	Ownership,

	/// An event emitted when an extended attribute of the file or folder is changed.
	///
	/// If the extended attribute's name or type is known, it should be provided in the
	/// `Info` event attribute.
	Extended,

	/// An event which specific kind is known but cannot be represented otherwise.
	Other,
}

/// An event emitted when the name of a file or folder is changed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum RenameMode {
	/// The catch-all case, to be used when the specific kind of event is unknown.
	Any,

	/// An event emitted on the file or folder resulting from a rename.
	To,

	/// An event emitted on the file or folder that was renamed.
	From,

	/// A single event emitted with both the `From` and `To` paths.
	///
	/// This event should be emitted when both source and target are known. The paths should be
	/// provided in this exact order (from, to).
	Both,

	/// An event which specific kind is known but cannot be represented otherwise.
	Other,
}

/// An event describing mutation of content, name, or metadata.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind", content = "mode"))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum ModifyKind {
	/// The catch-all case, to be used when the specific kind of event is unknown.
	Any,

	/// An event emitted when the data content of a file is changed.
	Data(DataChange),

	/// An event emitted when the metadata of a file or folder is changed.
	Metadata(MetadataKind),

	/// An event emitted when the name of a file or folder is changed.
	#[cfg_attr(feature = "serde", serde(rename = "rename"))]
	Name(RenameMode),

	/// An event which specific kind is known but cannot be represented otherwise.
	Other,
}

/// An event describing removal operations on files.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind"))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum RemoveKind {
	/// The catch-all case, to be used when the specific kind of event is unknown.
	Any,

	/// An event emitted when a file is removed.
	File,

	/// An event emitted when a folder is removed.
	Folder,

	/// An event which specific kind is known but cannot be represented otherwise.
	Other,
}

/// Top-level event kind.
///
/// This is arguably the most important classification for events. All subkinds below this one
/// represent details that may or may not be available for any particular backend, but most tools
/// and Notify systems will only care about which of these four general kinds an event is about.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum EventKind {
	/// The catch-all event kind, for unsupported/unknown events.
	///
	/// This variant should be used as the "else" case when mapping native kernel bitmasks or
	/// bitmaps, such that if the mask is ever extended with new event types the backend will not
	/// gain bugs due to not matching new unknown event types.
	///
	/// This variant is also the default variant used when Notify is in "imprecise" mode.
	Any,

	/// An event describing non-mutating access operations on files.
	///
	/// This event is about opening and closing file handles, as well as executing files, and any
	/// other such event that is about accessing files, folders, or other structures rather than
	/// mutating them.
	///
	/// Only some platforms are capable of generating these.
	Access(AccessKind),

	/// An event describing creation operations on files.
	///
	/// This event is about the creation of files, folders, or other structures but not about e.g.
	/// writing new content into them.
	Create(CreateKind),

	/// An event describing mutation of content, name, or metadata.
	///
	/// This event is about the mutation of files', folders', or other structures' content, name
	/// (path), or associated metadata (attributes).
	Modify(ModifyKind),

	/// An event describing removal operations on files.
	///
	/// This event is about the removal of files, folders, or other structures but not e.g. erasing
	/// content from them. This may also be triggered for renames/moves that move files _out of the
	/// watched subpath_.
	///
	/// Some editors also trigger Remove events when saving files as they may opt for removing (or
	/// renaming) the original then creating a new file in-place.
	Remove(RemoveKind),

	/// An event not fitting in any of the above four categories.
	///
	/// This may be used for meta-events about the watch itself.
	Other,
}

impl EventKind {
	/// Indicates whether an event is an Access variant.
	pub fn is_access(&self) -> bool {
		matches!(self, EventKind::Access(_))
	}

	/// Indicates whether an event is a Create variant.
	pub fn is_create(&self) -> bool {
		matches!(self, EventKind::Create(_))
	}

	/// Indicates whether an event is a Modify variant.
	pub fn is_modify(&self) -> bool {
		matches!(self, EventKind::Modify(_))
	}

	/// Indicates whether an event is a Remove variant.
	pub fn is_remove(&self) -> bool {
		matches!(self, EventKind::Remove(_))
	}

	/// Indicates whether an event is an Other variant.
	pub fn is_other(&self) -> bool {
		matches!(self, EventKind::Other)
	}
}

impl Default for EventKind {
	fn default() -> Self {
		EventKind::Any
	}
}
