use snapbox::assert_eq_path;
use watchexec_events::{Event, Keyboard, ProcessEnd, Source, Tag};

fn parse_file(path: &str) -> Vec<Event> {
	serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn single() {
	let single = Event {
		tags: vec![Tag::Source(Source::Internal)],
		metadata: Default::default(),
	};

	assert_eq_path(
		"tests/snapshots/single.json",
		serde_json::to_string_pretty(&single).unwrap(),
	);

	assert_eq!(
		serde_json::from_str::<Event>(
			&std::fs::read_to_string("tests/snapshots/single.json").unwrap()
		)
		.unwrap(),
		single
	);
}

#[test]
fn array() {
	let array = &[
		Event {
			tags: vec![Tag::Source(Source::Internal)],
			metadata: Default::default(),
		},
		Event {
			tags: vec![Tag::ProcessCompletion(Some(ProcessEnd::Success))],
			metadata: Default::default(),
		},
		Event {
			tags: vec![Tag::Keyboard(Keyboard::Eof)],
			metadata: Default::default(),
		},
	];

	assert_eq_path(
		"tests/snapshots/array.json",
		serde_json::to_string_pretty(array).unwrap(),
	);

	assert_eq!(parse_file("tests/snapshots/array.json"), array);
}

#[test]
fn sources() {
	let sources = &[
		Source::Filesystem,
		Source::Keyboard,
		Source::Mouse,
		Source::Os,
		Source::Time,
		Source::Internal,
	]
	.map(|source| Event {
		tags: vec![Tag::Source(source)],
		metadata: Default::default(),
	});

	assert_eq_path(
		"tests/snapshots/sources.json",
		serde_json::to_string_pretty(sources).unwrap(),
	);

	assert_eq!(parse_file("tests/snapshots/sources.json"), sources);
}
