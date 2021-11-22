use watchexec::{
	error::RuntimeError,
	event::{Event, FileType, Tag},
	filter::{globset::GlobsetFilterer, Filterer},
};

trait Harness {
	fn check_path(
		&self,
		path: &str,
		file_type: Option<FileType>,
	) -> std::result::Result<bool, RuntimeError>;

	fn path_pass(&self, path: &str, file_type: Option<FileType>, pass: bool) {
		assert_eq!(
			self.check_path(path, file_type).unwrap(),
			pass,
			"{} {:?} (expected {})",
			match file_type {
				Some(FileType::File) => "file",
				Some(FileType::Dir) => "dir",
				Some(FileType::Symlink) => "symlink",
				Some(FileType::Other) => "other",
				None => "path",
			},
			path,
			if pass { "pass" } else { "fail" }
		);
	}

	fn file_does_pass(&self, path: &str) {
		self.path_pass(path, Some(FileType::File), true);
	}

	fn file_doesnt_pass(&self, path: &str) {
		self.path_pass(path, Some(FileType::File), false);
	}

	fn dir_does_pass(&self, path: &str) {
		self.path_pass(path, Some(FileType::Dir), true);
	}

	fn dir_doesnt_pass(&self, path: &str) {
		self.path_pass(path, Some(FileType::Dir), false);
	}

	fn unk_does_pass(&self, path: &str) {
		self.path_pass(path, None, true);
	}

	fn unk_doesnt_pass(&self, path: &str) {
		self.path_pass(path, None, false);
	}
}

impl Harness for GlobsetFilterer {
	fn check_path(
		&self,
		path: &str,
		file_type: Option<FileType>,
	) -> std::result::Result<bool, RuntimeError> {
		let event = Event {
			tags: vec![Tag::Path {
				path: path.into(),
				file_type,
			}],
			metadata: Default::default(),
		};

		self.check_event(&event)
	}
}

#[test]
fn empty_filter_passes_everything() {
	let filterer = GlobsetFilterer::new("/test", vec![], vec![], vec![]).unwrap();

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/test/Cargo.toml");
	filterer.dir_does_pass("/a/folder");
	filterer.file_does_pass("apples/carrots/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples/carrots/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.dir_does_pass("apples/oranges/bananas");
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
	filterer.file_does_pass("/test/foo/bar/Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.dir_doesnt_pass("/a/folder");
	filterer.dir_does_pass("/test/Cargo.toml");
}

#[test]
fn exact_filenames_multiple() {
	let filterer = GlobsetFilterer::new(
		"/test",
		vec![
			("Cargo.toml".to_owned(), None),
			("package.json".to_owned(), None),
		],
		vec![],
		vec![],
	)
	.unwrap();

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("/test/foo/bar/Cargo.toml");
	filterer.file_does_pass("package.json");
	filterer.file_does_pass("/test/foo/bar/package.json");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_doesnt_pass("package.toml");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.dir_doesnt_pass("/a/folder");
	filterer.dir_does_pass("/test/Cargo.toml");
	filterer.dir_does_pass("/test/package.json");
}

#[test]
fn glob_single_final_ext_star() {
	let filterer =
		GlobsetFilterer::new("/test", vec![("Cargo.*".to_owned(), None)], vec![], vec![]).unwrap();

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.dir_doesnt_pass("/a/folder");
	filterer.dir_does_pass("Cargo.toml");
}

#[test]
fn glob_star_trailing_slash() {
	let filterer =
		GlobsetFilterer::new("/test", vec![("Cargo.*/".to_owned(), None)], vec![], vec![]).unwrap();

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.dir_doesnt_pass("/a/folder");
	filterer.dir_does_pass("Cargo.toml");
	filterer.unk_doesnt_pass("Cargo.toml");
}

#[test]
fn glob_star_leading_slash() {
	let filterer =
		GlobsetFilterer::new("/test", vec![("/Cargo.*".to_owned(), None)], vec![], vec![]).unwrap();

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.dir_does_pass("Cargo.toml");
	filterer.unk_does_pass("Cargo.toml");
	filterer.file_doesnt_pass("foo/Cargo.toml");
	filterer.dir_doesnt_pass("foo/Cargo.toml");
}

#[test]
fn glob_leading_double_star() {
	let filterer = GlobsetFilterer::new(
		"/test",
		vec![("**/possum".to_owned(), None)],
		vec![],
		vec![],
	)
	.unwrap();

	filterer.file_does_pass("possum");
	filterer.file_does_pass("foo/bar/possum");
	filterer.file_does_pass("/foo/bar/possum");
	filterer.dir_does_pass("possum");
	filterer.dir_does_pass("foo/bar/possum");
	filterer.dir_does_pass("/foo/bar/possum");
	filterer.file_doesnt_pass("rat");
	filterer.file_doesnt_pass("foo/bar/rat");
	filterer.file_doesnt_pass("/foo/bar/rat");
}

#[test]
fn glob_trailing_double_star() {
	let filterer = GlobsetFilterer::new(
		"/test",
		vec![("possum/**".to_owned(), None)],
		vec![],
		vec![],
	)
	.unwrap();

	filterer.file_doesnt_pass("possum");
	filterer.file_does_pass("possum/foo/bar");
	filterer.file_doesnt_pass("/possum/foo/bar");
	filterer.file_does_pass("/test/possum/foo/bar");
	filterer.dir_doesnt_pass("possum");
	filterer.dir_doesnt_pass("foo/bar/possum");
	filterer.dir_doesnt_pass("/foo/bar/possum");
	filterer.dir_does_pass("possum/foo/bar");
	filterer.dir_doesnt_pass("/possum/foo/bar");
	filterer.dir_does_pass("/test/possum/foo/bar");
	filterer.file_doesnt_pass("rat");
	filterer.file_doesnt_pass("foo/bar/rat");
	filterer.file_doesnt_pass("/foo/bar/rat");
}

#[test]
fn glob_middle_double_star() {
	let filterer = GlobsetFilterer::new(
		"/test",
		vec![("apples/**/oranges".to_owned(), None)],
		vec![],
		vec![],
	)
	.unwrap();

	filterer.dir_doesnt_pass("/a/folder");
	filterer.file_does_pass("apples/carrots/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_doesnt_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples/carrots/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.dir_doesnt_pass("apples/oranges/bananas");
}

#[test]
fn glob_double_star_trailing_slash() {
	let filterer = GlobsetFilterer::new(
		"/test",
		vec![("apples/**/oranges/".to_owned(), None)],
		vec![],
		vec![],
	)
	.unwrap();

	filterer.dir_doesnt_pass("/a/folder");
	filterer.file_doesnt_pass("apples/carrots/oranges");
	filterer.file_doesnt_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_doesnt_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_doesnt_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples/carrots/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.dir_doesnt_pass("apples/oranges/bananas");
	filterer.unk_doesnt_pass("apples/carrots/oranges");
	filterer.unk_doesnt_pass("apples/carrots/cauliflowers/oranges");
	filterer.unk_doesnt_pass("apples/carrots/cauliflowers/artichokes/oranges");
}

#[test]
fn ignore_exact_filename() {
	let filterer = GlobsetFilterer::new(
		"/test",
		vec![],
		vec![("Cargo.toml".to_owned(), None)],
		vec![],
	)
	.unwrap();

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("/test/foo/bar/Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.dir_doesnt_pass("/test/Cargo.toml");
}

#[test]
fn ignore_exact_filenames_multiple() {
	let filterer = GlobsetFilterer::new(
		"/test",
		vec![],
		vec![
			("Cargo.toml".to_owned(), None),
			("package.json".to_owned(), None),
		],
		vec![],
	)
	.unwrap();

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("/test/foo/bar/Cargo.toml");
	filterer.file_doesnt_pass("package.json");
	filterer.file_doesnt_pass("/test/foo/bar/package.json");
	filterer.file_does_pass("Cargo.json");
	filterer.file_does_pass("package.toml");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.dir_doesnt_pass("/test/Cargo.toml");
	filterer.dir_doesnt_pass("/test/package.json");
}

#[test]
fn ignore_glob_single_final_ext_star() {
	let filterer =
		GlobsetFilterer::new("/test", vec![], vec![("Cargo.*".to_owned(), None)], vec![]).unwrap();

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.dir_doesnt_pass("Cargo.toml");
}

#[test]
fn ignore_glob_star_trailing_slash() {
	let filterer =
		GlobsetFilterer::new("/test", vec![], vec![("Cargo.*/".to_owned(), None)], vec![]).unwrap();

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.dir_doesnt_pass("Cargo.toml");
	filterer.unk_does_pass("Cargo.toml");
}

#[test]
fn ignore_glob_star_leading_slash() {
	let filterer =
		GlobsetFilterer::new("/test", vec![], vec![("/Cargo.*".to_owned(), None)], vec![]).unwrap();

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.dir_doesnt_pass("Cargo.toml");
	filterer.unk_doesnt_pass("Cargo.toml");
	filterer.file_does_pass("foo/Cargo.toml");
	filterer.dir_does_pass("foo/Cargo.toml");
}

#[test]
fn ignore_glob_leading_double_star() {
	let filterer = GlobsetFilterer::new(
		"/test",
		vec![],
		vec![("**/possum".to_owned(), None)],
		vec![],
	)
	.unwrap();

	filterer.file_doesnt_pass("possum");
	filterer.file_doesnt_pass("foo/bar/possum");
	filterer.file_doesnt_pass("/foo/bar/possum");
	filterer.dir_doesnt_pass("possum");
	filterer.dir_doesnt_pass("foo/bar/possum");
	filterer.dir_doesnt_pass("/foo/bar/possum");
	filterer.file_does_pass("rat");
	filterer.file_does_pass("foo/bar/rat");
	filterer.file_does_pass("/foo/bar/rat");
}

#[test]
fn ignore_glob_trailing_double_star() {
	let filterer = GlobsetFilterer::new(
		"/test",
		vec![],
		vec![("possum/**".to_owned(), None)],
		vec![],
	)
	.unwrap();

	filterer.file_does_pass("possum");
	filterer.file_doesnt_pass("possum/foo/bar");
	filterer.file_does_pass("/possum/foo/bar");
	filterer.file_doesnt_pass("/test/possum/foo/bar");
	filterer.dir_does_pass("possum");
	filterer.dir_does_pass("foo/bar/possum");
	filterer.dir_does_pass("/foo/bar/possum");
	filterer.dir_doesnt_pass("possum/foo/bar");
	filterer.dir_does_pass("/possum/foo/bar");
	filterer.dir_doesnt_pass("/test/possum/foo/bar");
	filterer.file_does_pass("rat");
	filterer.file_does_pass("foo/bar/rat");
	filterer.file_does_pass("/foo/bar/rat");
}

#[test]
fn ignore_glob_middle_double_star() {
	let filterer = GlobsetFilterer::new(
		"/test",
		vec![],
		vec![("apples/**/oranges".to_owned(), None)],
		vec![],
	)
	.unwrap();

	filterer.dir_does_pass("/a/folder");
	filterer.file_doesnt_pass("apples/carrots/oranges");
	filterer.file_doesnt_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_doesnt_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_doesnt_pass("apples/carrots/oranges");
	filterer.dir_doesnt_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_doesnt_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.dir_does_pass("apples/oranges/bananas");
}

#[test]
fn ignore_glob_double_star_trailing_slash() {
	let filterer = GlobsetFilterer::new(
		"/test",
		vec![],
		vec![("apples/**/oranges/".to_owned(), None)],
		vec![],
	)
	.unwrap();

	filterer.dir_does_pass("/a/folder");
	filterer.file_does_pass("apples/carrots/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_doesnt_pass("apples/carrots/oranges");
	filterer.dir_doesnt_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_doesnt_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.dir_does_pass("apples/oranges/bananas");
	filterer.unk_does_pass("apples/carrots/oranges");
	filterer.unk_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.unk_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
}

#[test]
fn ignores_take_precedence() {
	let filterer = GlobsetFilterer::new(
		"/test",
		vec![
			("*.docx".to_owned(), None),
			("*.toml".to_owned(), None),
			("*.json".to_owned(), None),
		],
		vec![("*.toml".to_owned(), None), ("*.json".to_owned(), None)],
		vec![],
	)
	.unwrap();

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("/test/foo/bar/Cargo.toml");
	filterer.file_doesnt_pass("package.json");
	filterer.file_doesnt_pass("/test/foo/bar/package.json");
	filterer.dir_doesnt_pass("/test/Cargo.toml");
	filterer.dir_doesnt_pass("/test/package.json");
	filterer.file_does_pass("FINAL-FINAL.docx");
}

// The following tests replicate the "buggy"/"confusing" watchexec v1 behaviour.

#[test]
fn ignore_folder_incorrectly_with_bare_match() {
	let filterer =
		GlobsetFilterer::new("/test", vec![], vec![("prunes".to_owned(), None)], vec![]).unwrap();

	filterer.file_does_pass("apples");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");

	filterer.file_does_pass("raw-prunes");
	filterer.dir_does_pass("raw-prunes");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("raw-prunes/oranges/bananas");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");

	filterer.file_doesnt_pass("prunes");
	filterer.dir_doesnt_pass("prunes");

	// buggy behaviour (should be doesnt):
	filterer.file_does_pass("prunes/carrots/cauliflowers/oranges");
	filterer.file_does_pass("prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("prunes/oranges/bananas");
	filterer.dir_does_pass("prunes/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("prunes/carrots/cauliflowers/artichokes/oranges");
}

#[test]
fn ignore_folder_incorrectly_with_bare_and_leading_slash() {
	let filterer =
		GlobsetFilterer::new("/test", vec![], vec![("/prunes".to_owned(), None)], vec![]).unwrap();

	filterer.file_does_pass("apples");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");

	filterer.file_does_pass("raw-prunes");
	filterer.dir_does_pass("raw-prunes");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("raw-prunes/oranges/bananas");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");

	filterer.file_doesnt_pass("prunes");
	filterer.dir_doesnt_pass("prunes");

	// buggy behaviour (should be doesnt):
	filterer.file_does_pass("prunes/carrots/cauliflowers/oranges");
	filterer.file_does_pass("prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("prunes/oranges/bananas");
	filterer.dir_does_pass("prunes/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("prunes/carrots/cauliflowers/artichokes/oranges");
}

#[test]
fn ignore_folder_incorrectly_with_bare_and_trailing_slash() {
	let filterer =
		GlobsetFilterer::new("/test", vec![], vec![("prunes/".to_owned(), None)], vec![]).unwrap();

	filterer.file_does_pass("apples");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");

	filterer.file_does_pass("raw-prunes");
	filterer.dir_does_pass("raw-prunes");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("raw-prunes/oranges/bananas");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");

	filterer.dir_doesnt_pass("prunes");

	// buggy behaviour (should be doesnt):
	filterer.file_does_pass("prunes");
	filterer.file_does_pass("prunes/carrots/cauliflowers/oranges");
	filterer.file_does_pass("prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("prunes/oranges/bananas");
	filterer.dir_does_pass("prunes/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("prunes/carrots/cauliflowers/artichokes/oranges");
}

#[test]
fn ignore_folder_incorrectly_with_only_double_double_glob() {
	let filterer = GlobsetFilterer::new(
		"/test",
		vec![],
		vec![("**/prunes/**".to_owned(), None)],
		vec![],
	)
	.unwrap();

	filterer.file_does_pass("apples");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");

	filterer.file_does_pass("raw-prunes");
	filterer.dir_does_pass("raw-prunes");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("raw-prunes/oranges/bananas");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");

	filterer.file_doesnt_pass("prunes/carrots/cauliflowers/oranges");
	filterer.file_doesnt_pass("prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_doesnt_pass("prunes/oranges/bananas");
	filterer.dir_doesnt_pass("prunes/carrots/cauliflowers/oranges");
	filterer.dir_doesnt_pass("prunes/carrots/cauliflowers/artichokes/oranges");

	// buggy behaviour (should be doesnt):
	filterer.file_does_pass("prunes");
	filterer.dir_does_pass("prunes");
}

#[test]
fn ignore_folder_correctly_with_double_and_double_double_globs() {
	let filterer = GlobsetFilterer::new(
		"/test",
		vec![],
		vec![
			("**/prunes".to_owned(), None),
			("**/prunes/**".to_owned(), None),
		],
		vec![],
	)
	.unwrap();

	filterer.file_does_pass("apples");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");

	filterer.file_does_pass("raw-prunes");
	filterer.dir_does_pass("raw-prunes");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.file_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_does_pass("raw-prunes/oranges/bananas");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("raw-prunes/carrots/cauliflowers/artichokes/oranges");

	filterer.file_doesnt_pass("prunes");
	filterer.file_doesnt_pass("prunes/carrots/cauliflowers/oranges");
	filterer.file_doesnt_pass("prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_doesnt_pass("prunes/oranges/bananas");
	filterer.dir_doesnt_pass("prunes");
	filterer.dir_doesnt_pass("prunes/carrots/cauliflowers/oranges");
	filterer.dir_doesnt_pass("prunes/carrots/cauliflowers/artichokes/oranges");
}
