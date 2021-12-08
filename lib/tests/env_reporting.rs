use std::{
	collections::HashMap,
	ffi::{OsStr, OsString},
};

use notify::event::CreateKind;
use watchexec::{
	event::{filekind::*, Event, Tag},
	paths::summarise_events_to_env,
};

#[cfg(unix)]
const ENV_SEP: &str = ":";
#[cfg(not(unix))]
const ENV_SEP: &str = ";";

fn ospath(path: &str) -> OsString {
	let root = dunce::canonicalize(".").unwrap();
	if path.is_empty() {
		root
	} else {
		root.join(path)
	}
	.into()
}

fn event(path: &str, kind: FileEventKind) -> Event {
	tracing_subscriber::fmt::try_init().ok();
	let path = dunce::canonicalize(".").unwrap().join(path);
	Event {
		tags: vec![
			Tag::Path {
				path,
				file_type: None,
			},
			Tag::FileEventKind(kind),
		],
		metadata: Default::default(),
	}
}

#[test]
fn no_events_no_env() {
	let events = Vec::<Event>::new();
	assert_eq!(summarise_events_to_env(&events), HashMap::new());
}

#[test]
fn single_created() {
	let events = vec![event("file.txt", FileEventKind::Create(CreateKind::File))];
	assert_eq!(
		summarise_events_to_env(&events),
		HashMap::from([
			(OsStr::new("CREATED"), OsString::from("file.txt")),
			(OsStr::new("COMMON_PATH"), ospath("")),
		])
	);
}

#[test]
fn single_meta() {
	let events = vec![event(
		"file.txt",
		FileEventKind::Modify(ModifyKind::Metadata(MetadataKind::Any)),
	)];
	assert_eq!(
		summarise_events_to_env(&events),
		HashMap::from([
			(OsStr::new("META_CHANGED"), OsString::from("file.txt")),
			(OsStr::new("COMMON_PATH"), ospath("")),
		])
	);
}

#[test]
fn single_removed() {
	let events = vec![event("file.txt", FileEventKind::Remove(RemoveKind::File))];
	assert_eq!(
		summarise_events_to_env(&events),
		HashMap::from([
			(OsStr::new("REMOVED"), OsString::from("file.txt")),
			(OsStr::new("COMMON_PATH"), ospath("")),
		])
	);
}

#[test]
fn single_renamed() {
	let events = vec![event(
		"file.txt",
		FileEventKind::Modify(ModifyKind::Name(RenameMode::Any)),
	)];
	assert_eq!(
		summarise_events_to_env(&events),
		HashMap::from([
			(OsStr::new("RENAMED"), OsString::from("file.txt")),
			(OsStr::new("COMMON_PATH"), ospath("")),
		])
	);
}

#[test]
fn single_written() {
	let events = vec![event(
		"file.txt",
		FileEventKind::Modify(ModifyKind::Data(DataChange::Any)),
	)];
	assert_eq!(
		summarise_events_to_env(&events),
		HashMap::from([
			(OsStr::new("WRITTEN"), OsString::from("file.txt")),
			(OsStr::new("COMMON_PATH"), ospath("")),
		])
	);
}

#[test]
fn single_otherwise() {
	let events = vec![event("file.txt", FileEventKind::Any)];
	assert_eq!(
		summarise_events_to_env(&events),
		HashMap::from([
			(OsStr::new("OTHERWISE_CHANGED"), OsString::from("file.txt")),
			(OsStr::new("COMMON_PATH"), ospath("")),
		])
	);
}

#[test]
fn all_types_once() {
	let events = vec![
		event("create.txt", FileEventKind::Create(CreateKind::File)),
		event(
			"metadata.txt",
			FileEventKind::Modify(ModifyKind::Metadata(MetadataKind::Any)),
		),
		event("remove.txt", FileEventKind::Remove(RemoveKind::File)),
		event(
			"rename.txt",
			FileEventKind::Modify(ModifyKind::Name(RenameMode::Any)),
		),
		event(
			"modify.txt",
			FileEventKind::Modify(ModifyKind::Data(DataChange::Any)),
		),
		event("any.txt", FileEventKind::Any),
	];
	assert_eq!(
		summarise_events_to_env(&events),
		HashMap::from([
			(OsStr::new("CREATED"), OsString::from("create.txt")),
			(OsStr::new("META_CHANGED"), OsString::from("metadata.txt")),
			(OsStr::new("REMOVED"), OsString::from("remove.txt")),
			(OsStr::new("RENAMED"), OsString::from("rename.txt")),
			(OsStr::new("WRITTEN"), OsString::from("modify.txt")),
			(OsStr::new("OTHERWISE_CHANGED"), OsString::from("any.txt")),
			(OsStr::new("COMMON_PATH"), ospath("")),
		])
	);
}

#[test]
fn single_type_multipath() {
	let events = vec![
		event("root.txt", FileEventKind::Create(CreateKind::File)),
		event("sub/folder.txt", FileEventKind::Create(CreateKind::File)),
		event("dom/folder.txt", FileEventKind::Create(CreateKind::File)),
		event(
			"deeper/sub/folder.txt",
			FileEventKind::Create(CreateKind::File),
		),
	];
	assert_eq!(
		summarise_events_to_env(&events),
		HashMap::from([
			(
				OsStr::new("CREATED"),
				OsString::from(
					"".to_string()
						+ "root.txt" + ENV_SEP + "sub/folder.txt"
						+ ENV_SEP + "dom/folder.txt"
						+ ENV_SEP + "deeper/sub/folder.txt"
				)
			),
			(OsStr::new("COMMON_PATH"), ospath("")),
		])
	);
}

#[test]
fn single_type_divergent_paths() {
	let events = vec![
		event("sub/folder.txt", FileEventKind::Create(CreateKind::File)),
		event("dom/folder.txt", FileEventKind::Create(CreateKind::File)),
	];
	assert_eq!(
		summarise_events_to_env(&events),
		HashMap::from([
			(
				OsStr::new("CREATED"),
				OsString::from("".to_string() + "sub/folder.txt" + ENV_SEP + "dom/folder.txt")
			),
			(OsStr::new("COMMON_PATH"), ospath("")),
		])
	);
}

#[test]
fn multitype_multipath() {
	let events = vec![
		event("root.txt", FileEventKind::Create(CreateKind::File)),
		event("sibling.txt", FileEventKind::Create(CreateKind::Any)),
		event(
			"sub/folder.txt",
			FileEventKind::Modify(ModifyKind::Metadata(MetadataKind::Ownership)),
		),
		event("dom/folder.txt", FileEventKind::Remove(RemoveKind::Folder)),
		event("deeper/sub/folder.txt", FileEventKind::Other),
	];
	assert_eq!(
		summarise_events_to_env(&events),
		HashMap::from([
			(
				OsStr::new("CREATED"),
				OsString::from("".to_string() + "root.txt" + ENV_SEP + "sibling.txt"),
			),
			(OsStr::new("META_CHANGED"), OsString::from("sub/folder.txt"),),
			(OsStr::new("REMOVED"), OsString::from("dom/folder.txt"),),
			(
				OsStr::new("OTHERWISE_CHANGED"),
				OsString::from("deeper/sub/folder.txt"),
			),
			(OsStr::new("COMMON_PATH"), ospath("")),
		])
	);
}
