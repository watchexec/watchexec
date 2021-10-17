use watchexec::{
	error::RuntimeError,
	event::{Event, Tag},
	filter::{globset::GlobsetFilterer, Filterer},
};

trait Harness {
	fn check_path(&self, path: &str) -> std::result::Result<bool, RuntimeError>;

	fn does_pass(&self, path: &str) {
		assert!(
			matches!(self.check_path(path), Ok(true)),
			"path {:?} (expected pass)",
			path
		);
	}

	fn doesnt_pass(&self, path: &str) {
		assert!(
			matches!(self.check_path(path), Ok(false)),
			"path {:?} (expected fail)",
			path
		);
	}
}

impl Harness for GlobsetFilterer {
	fn check_path(&self, path: &str) -> std::result::Result<bool, RuntimeError> {
		let event = Event {
			tags: vec![Tag::Path {
				path: path.into(),
				file_type: None,
			}],
			metadata: Default::default(),
		};

		self.check_event(&event)
	}
}

#[test]
fn exact_filename() {
	let filterer = GlobsetFilterer::new(
		"/test",
		vec![("Cargo.toml".to_owned(), None)],
		vec![],
		vec![],
	)
	.unwrap();

	filterer.does_pass("Cargo.toml");
	filterer.doesnt_pass("Cargo.json");
	filterer.doesnt_pass("Gemfile.toml");
	filterer.doesnt_pass("FINAL-FINAL.docx");
	filterer.doesnt_pass("/a/folder");
}
