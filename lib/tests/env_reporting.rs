use std::{
	collections::HashMap,
	ffi::{OsStr, OsString},
};

use notify::event::CreateKind;
use watchexec::{
	event::{filekind::*, Event, Tag},
	paths::summarise_events_to_env,
};

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
