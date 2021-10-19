use watchexec::{
	error::RuntimeError,
	event::{Event, Tag},
	filter::{globset::GlobsetFilterer, Filterer},
};

trait Harness {
	fn check_path(&self, path: &str) -> std::result::Result<bool, RuntimeError>;

	fn file_does_pass(&self, path: &str) {
		assert!(
			matches!(self.check_path(path), Ok(true)),
			"path {:?} (expected pass)",
			path
		);
	}

	fn file_doesnt_pass(&self, path: &str) {
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

	filterer.file_does_pass("Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.file_doesnt_pass("/a/folder");
}

#[test]
fn glob_filename() {
	let filterer =
		GlobsetFilterer::new("/test", vec![("Cargo.*".to_owned(), None)], vec![], vec![]).unwrap();

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.file_doesnt_pass("/a/folder");
}
