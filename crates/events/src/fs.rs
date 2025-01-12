use std::fmt;

/// Re-export of the Notify file event types.
#[cfg(feature = "notify")]
pub mod filekind {
	pub use notify_types::event::{
		AccessKind, AccessMode, CreateKind, DataChange, EventKind as FileEventKind, MetadataKind,
		ModifyKind, RemoveKind, RenameMode,
	};
}

/// Pseudo file event types without dependency on Notify.
#[cfg(not(feature = "notify"))]
pub mod filekind {
	pub use crate::sans_notify::{
		AccessKind, AccessMode, CreateKind, DataChange, EventKind as FileEventKind, MetadataKind,
		ModifyKind, RemoveKind, RenameMode,
	};
}

/// The type of a file.
///
/// This is a simplification of the [`std::fs::FileType`] type, which is not constructable and may
/// differ on different platforms.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
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
