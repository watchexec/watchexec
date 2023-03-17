use snapbox::assert_eq_path;
use watchexec_events::{Event, Source, Tag};

#[test]
fn generate_single() {
	let event = Event {
		tags: vec![Tag::Source(Source::Internal)],
		metadata: Default::default(),
	};
	assert_eq_path(
		"tests/snapshots/generate_single.json",
		serde_json::to_string_pretty(&event).unwrap(),
	);
}
