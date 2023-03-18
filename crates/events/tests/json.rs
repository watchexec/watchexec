use std::num::{NonZeroI32, NonZeroI64};

use snapbox::assert_eq_path;
use watchexec_events::{Event, Keyboard, ProcessEnd, Source, Tag};
use watchexec_signals::Signal;

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
			tags: vec![
				Tag::ProcessCompletion(Some(ProcessEnd::Success)),
				Tag::Process(123),
			],
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
	let sources = vec![
		Event {
			tags: vec![
				Tag::Source(Source::Filesystem),
				Tag::Source(Source::Keyboard),
				Tag::Source(Source::Mouse),
			],
			metadata: Default::default(),
		},
		Event {
			tags: vec![
				Tag::Source(Source::Os),
				Tag::Source(Source::Time),
				Tag::Source(Source::Internal),
			],
			metadata: Default::default(),
		},
	];

	assert_eq_path(
		"tests/snapshots/sources.json",
		serde_json::to_string_pretty(&sources).unwrap(),
	);

	assert_eq!(parse_file("tests/snapshots/sources.json"), sources);
}

#[test]
fn signals() {
	let signals = vec![
		Event {
			tags: vec![
				Tag::Signal(Signal::Interrupt),
				Tag::Signal(Signal::User1),
				Tag::Signal(Signal::ForceStop),
			],
			metadata: Default::default(),
		},
		Event {
			tags: vec![
				Tag::Signal(Signal::Custom(66)),
				Tag::Signal(Signal::Custom(0)),
			],
			metadata: Default::default(),
		},
	];

	assert_eq_path(
		"tests/snapshots/signals.json",
		serde_json::to_string_pretty(&signals).unwrap(),
	);

	assert_eq!(parse_file("tests/snapshots/signals.json"), signals);
}

#[test]
fn completions() {
	let completions = vec![
		Event {
			tags: vec![
				Tag::ProcessCompletion(None),
				Tag::ProcessCompletion(Some(ProcessEnd::Success)),
				Tag::ProcessCompletion(Some(ProcessEnd::Continued)),
			],
			metadata: Default::default(),
		},
		Event {
			tags: vec![
				Tag::ProcessCompletion(Some(ProcessEnd::ExitError(NonZeroI64::new(12).unwrap()))),
				Tag::ProcessCompletion(Some(ProcessEnd::ExitSignal(Signal::Interrupt))),
				Tag::ProcessCompletion(Some(ProcessEnd::ExitSignal(Signal::Custom(34)))),
				Tag::ProcessCompletion(Some(ProcessEnd::Exception(NonZeroI32::new(56).unwrap()))),
			],
			metadata: Default::default(),
		},
	];

	assert_eq_path(
		"tests/snapshots/completions.json",
		serde_json::to_string_pretty(&completions).unwrap(),
	);

	assert_eq!(parse_file("tests/snapshots/completions.json"), completions);
}
