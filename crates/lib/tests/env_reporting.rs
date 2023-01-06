use std::{collections::HashMap, ffi::OsString};

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
	Event {
		tags: vec![
			Tag::Path {
				path: ospath(path).into(),
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
			("CREATED", OsString::from("file.txt")),
			("COMMON", ospath("")),
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
			("META_CHANGED", OsString::from("file.txt")),
			("COMMON", ospath("")),
		])
	);
}

#[test]
fn single_removed() {
	let events = vec![event("file.txt", FileEventKind::Remove(RemoveKind::File))];
	assert_eq!(
		summarise_events_to_env(&events),
		HashMap::from([
			("REMOVED", OsString::from("file.txt")),
			("COMMON", ospath("")),
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
			("RENAMED", OsString::from("file.txt")),
			("COMMON", ospath("")),
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
			("WRITTEN", OsString::from("file.txt")),
			("COMMON", ospath("")),
		])
	);
}

#[test]
fn single_otherwise() {
	let events = vec![event("file.txt", FileEventKind::Any)];
	assert_eq!(
		summarise_events_to_env(&events),
		HashMap::from([
			("OTHERWISE_CHANGED", OsString::from("file.txt")),
			("COMMON", ospath("")),
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
			("CREATED", OsString::from("create.txt")),
			("META_CHANGED", OsString::from("metadata.txt")),
			("REMOVED", OsString::from("remove.txt")),
			("RENAMED", OsString::from("rename.txt")),
			("WRITTEN", OsString::from("modify.txt")),
			("OTHERWISE_CHANGED", OsString::from("any.txt")),
			("COMMON", ospath("")),
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
				"CREATED",
				OsString::from(
					String::new()
						+ "deeper/sub/folder.txt"
						+ ENV_SEP + "dom/folder.txt"
						+ ENV_SEP + "root.txt" + ENV_SEP
						+ "sub/folder.txt"
				)
			),
			("COMMON", ospath("")),
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
				"CREATED",
				OsString::from(String::new() + "dom/folder.txt" + ENV_SEP + "sub/folder.txt")
			),
			("COMMON", ospath("")),
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
				"CREATED",
				OsString::from(String::new() + "root.txt" + ENV_SEP + "sibling.txt"),
			),
			("META_CHANGED", OsString::from("sub/folder.txt"),),
			("REMOVED", OsString::from("dom/folder.txt"),),
			("OTHERWISE_CHANGED", OsString::from("deeper/sub/folder.txt"),),
			("COMMON", ospath("")),
		])
	);
}

#[test]
fn multiple_paths_in_one_event() {
	let events = vec![Event {
		tags: vec![
			Tag::Path {
				path: ospath("one.txt").into(),
				file_type: None,
			},
			Tag::Path {
				path: ospath("two.txt").into(),
				file_type: None,
			},
			Tag::FileEventKind(FileEventKind::Any),
		],
		metadata: Default::default(),
	}];
	assert_eq!(
		summarise_events_to_env(&events),
		HashMap::from([
			(
				"OTHERWISE_CHANGED",
				OsString::from(String::new() + "one.txt" + ENV_SEP + "two.txt")
			),
			("COMMON", ospath("")),
		])
	);
}

#[test]
fn mixed_non_paths_events() {
	let events = vec![
		event("one.txt", FileEventKind::Any),
		Event {
			tags: vec![Tag::Process(1234)],
			metadata: Default::default(),
		},
		event("two.txt", FileEventKind::Any),
		Event {
			tags: vec![Tag::FileEventKind(FileEventKind::Any)],
			metadata: Default::default(),
		},
	];
	assert_eq!(
		summarise_events_to_env(&events),
		HashMap::from([
			(
				"OTHERWISE_CHANGED",
				OsString::from(String::new() + "one.txt" + ENV_SEP + "two.txt")
			),
			("COMMON", ospath("")),
		])
	);
}

#[test]
fn only_non_paths_events() {
	let events = vec![
		Event {
			tags: vec![Tag::Process(1234)],
			metadata: Default::default(),
		},
		Event {
			tags: vec![Tag::FileEventKind(FileEventKind::Any)],
			metadata: Default::default(),
		},
	];
	assert_eq!(summarise_events_to_env(&events), HashMap::new());
}

#[test]
fn multipath_is_sorted() {
	let events = vec![
		event("0123.txt", FileEventKind::Any),
		event("a.txt", FileEventKind::Any),
		event("b.txt", FileEventKind::Any),
		event("c.txt", FileEventKind::Any),
		event("ᄁ.txt", FileEventKind::Any),
	];
	assert_eq!(
		summarise_events_to_env(&events),
		HashMap::from([
			(
				"OTHERWISE_CHANGED",
				OsString::from(
					String::new()
						+ "0123.txt" + ENV_SEP + "a.txt"
						+ ENV_SEP + "b.txt" + ENV_SEP
						+ "c.txt" + ENV_SEP + "ᄁ.txt"
				)
			),
			("COMMON", ospath("")),
		])
	);
}

#[test]
fn multipath_is_deduped() {
	let events = vec![
		event("0123.txt", FileEventKind::Any),
		event("0123.txt", FileEventKind::Any),
		event("a.txt", FileEventKind::Any),
		event("a.txt", FileEventKind::Any),
		event("b.txt", FileEventKind::Any),
		event("b.txt", FileEventKind::Any),
		event("c.txt", FileEventKind::Any),
		event("ᄁ.txt", FileEventKind::Any),
		event("ᄁ.txt", FileEventKind::Any),
	];
	assert_eq!(
		summarise_events_to_env(&events),
		HashMap::from([
			(
				"OTHERWISE_CHANGED",
				OsString::from(
					String::new()
						+ "0123.txt" + ENV_SEP + "a.txt"
						+ ENV_SEP + "b.txt" + ENV_SEP
						+ "c.txt" + ENV_SEP + "ᄁ.txt"
				)
			),
			("COMMON", ospath("")),
		])
	);
}
