use std::{ffi::OsString, path::PathBuf};

use watchexec::{
	error::RuntimeError,
	event::{Event, FileType, Tag},
	filter::{globset::GlobsetFilterer, Filterer},
};

trait Harness {
	fn check_path(
		&self,
		path: PathBuf,
		file_type: Option<FileType>,
	) -> std::result::Result<bool, RuntimeError>;

	fn path_pass(&self, path: &str, file_type: Option<FileType>, pass: bool) {
		let origin = dunce::canonicalize(".").unwrap();
		let full_path = if let Some(suf) = path.strip_prefix("/test/") {
			origin.join(suf)
		} else {
			origin.join(path)
		};

		assert_eq!(
			self.check_path(full_path, file_type).unwrap(),
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
		path: PathBuf,
		file_type: Option<FileType>,
	) -> std::result::Result<bool, RuntimeError> {
		let event = Event {
			tags: vec![Tag::Path { path, file_type }],
			metadata: Default::default(),
		};

		self.check_event(&event)
	}
}

fn filt(filters: &[&str], ignores: &[&str], extensions: &[&str]) -> GlobsetFilterer {
	let origin = dunce::canonicalize(".").unwrap();
	GlobsetFilterer::new(
		origin,
		filters.iter().map(|s| (s.to_string(), None)),
		ignores.iter().map(|s| (s.to_string(), None)),
		extensions.iter().map(OsString::from),
	)
	.expect("making filterer")
}

#[test]
fn empty_filter_passes_everything() {
	let filterer = filt(&[], &[], &[]);

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
	let filterer = filt(&["Cargo.toml"], &[], &[]);

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
	let filterer = filt(&["Cargo.toml", "package.json"], &[], &[]);

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
	let filterer = filt(&["Cargo.*"], &[], &[]);

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.dir_doesnt_pass("/a/folder");
	filterer.dir_does_pass("Cargo.toml");
}

#[test]
fn glob_star_trailing_slash() {
	let filterer = filt(&["Cargo.*/"], &[], &[]);

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
	let filterer = filt(&["/Cargo.*"], &[], &[]);

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.dir_does_pass("Cargo.toml");
	filterer.unk_does_pass("Cargo.toml");
	filterer.file_doesnt_pass("foo/Cargo.toml");
	filterer.dir_doesnt_pass("foo/Cargo.toml");
}

#[test]
fn glob_leading_double_star() {
	let filterer = filt(&["**/possum"], &[], &[]);

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
	let filterer = filt(&["possum/**"], &[], &[]);

	// these do work by expectation and in v1
	filterer.file_does_pass("/test/possum/foo/bar");
	filterer.dir_doesnt_pass("possum");
	filterer.dir_doesnt_pass("foo/bar/possum");
	filterer.dir_does_pass("possum/foo/bar");
	filterer.file_doesnt_pass("rat");
	filterer.file_doesnt_pass("foo/bar/rat");
	filterer.file_doesnt_pass("/foo/bar/rat");
}

#[test]
fn glob_middle_double_star() {
	let filterer = filt(&["apples/**/oranges"], &[], &[]);

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
	let filterer = filt(&["apples/**/oranges/"], &[], &[]);

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
	let filterer = filt(&[], &["Cargo.toml"], &[]);

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
	let filterer = filt(&[], &["Cargo.toml", "package.json"], &[]);

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
	let filterer = filt(&[], &["Cargo.*"], &[]);

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.dir_doesnt_pass("Cargo.toml");
}

#[test]
fn ignore_glob_star_trailing_slash() {
	let filterer = filt(&[], &["Cargo.*/"], &[]);

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
	let filterer = filt(&[], &["/Cargo.*"], &[]);

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.dir_doesnt_pass("Cargo.toml");
	filterer.unk_doesnt_pass("Cargo.toml");
	filterer.file_does_pass("foo/Cargo.toml");
	filterer.dir_does_pass("foo/Cargo.toml");
}

#[test]
fn ignore_glob_leading_double_star() {
	let filterer = filt(&[], &["**/possum"], &[]);

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
	let filterer = filt(&[], &["possum/**"], &[]);

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
	let filterer = filt(&[], &["apples/**/oranges"], &[]);

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
	let filterer = filt(&[], &["apples/**/oranges/"], &[]);

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
	let filterer = filt(&["*.docx", "*.toml", "*.json"], &["*.toml", "*.json"], &[]);

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
	let filterer = filt(&[], &["prunes"], &[]);

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
	let filterer = filt(&[], &["/prunes"], &[]);

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
	let filterer = filt(&[], &["prunes/"], &[]);

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
	let filterer = filt(&[], &["**/prunes/**"], &[]);

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
	let filterer = filt(&[], &["**/prunes", "**/prunes/**"], &[]);

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
