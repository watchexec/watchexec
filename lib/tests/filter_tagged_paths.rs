use std::{
	path::{Path, PathBuf},
	sync::Arc,
};

use watchexec::{
	error::RuntimeError,
	event::{Event, FileType, Tag},
	filter::{
		tagged::{Filter, Matcher, Op, Pattern, TaggedFilterer},
		Filterer,
	},
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
		} else if Path::new(path).has_root() {
			path.into()
		} else {
			origin.join(path)
		};

		tracing::info!(?path, ?file_type, ?pass, "check");

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

impl Harness for TaggedFilterer {
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

async fn filt(filters: &[Filter]) -> Arc<TaggedFilterer> {
	let origin = dunce::canonicalize(".").unwrap();
	let filterer = TaggedFilterer::new(origin.clone(), origin).expect("creating filterer");
	filterer.add_filters(filters).await.expect("adding filters");
	tracing_subscriber::fmt::try_init().ok();
	filterer
}

fn filter(pat: &str) -> Filter {
	Filter {
		in_path: None,
		on: Matcher::Path,
		op: Op::Glob,
		pat: Pattern::Glob(pat.into()),
		negate: false,
	}
}

fn not_filter(pat: &str) -> Filter {
	Filter {
		in_path: None,
		on: Matcher::Path,
		op: Op::NotGlob,
		pat: Pattern::Glob(pat.into()),
		negate: false,
	}
}

trait FilterExt {
	fn in_path(self) -> Self
	where
		Self: Sized,
	{
		self.in_subpath("")
	}

	fn in_subpath(self, sub: impl AsRef<Path>) -> Self;
}

impl FilterExt for Filter {
	fn in_subpath(mut self, sub: impl AsRef<Path>) -> Self {
		let origin = dunce::canonicalize(".").unwrap();
		self.in_path = Some(origin.join(sub));
		self
	}
}

#[tokio::test]
async fn empty_filter_passes_everything() {
	let filterer = filt(&[]).await;

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

#[tokio::test]
async fn exact_filename() {
	let filterer = filt(&[filter("Cargo.toml")]).await;

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("/test/foo/bar/Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.dir_doesnt_pass("/a/folder");
	filterer.dir_does_pass("/test/Cargo.toml");
}

#[tokio::test]
async fn exact_filenames_multiple() {
	let filterer = filt(&[filter("Cargo.toml"), filter("package.json")]).await;

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

#[tokio::test]
async fn glob_single_final_ext_star() {
	let filterer = filt(&[filter("Cargo.*")]).await;

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.dir_doesnt_pass("/a/folder");
	filterer.dir_does_pass("Cargo.toml");
}

#[tokio::test]
async fn glob_star_trailing_slash() {
	let filterer = filt(&[filter("Cargo.*/")]).await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.dir_doesnt_pass("/a/folder");
	filterer.dir_does_pass("Cargo.toml");
	filterer.unk_doesnt_pass("Cargo.toml");
}

#[tokio::test]
async fn glob_star_leading_slash() {
	let filterer = filt(&[filter("/Cargo.*")]).await;

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.dir_does_pass("Cargo.toml");
	filterer.unk_does_pass("Cargo.toml");
	filterer.file_doesnt_pass("foo/Cargo.toml");
	filterer.dir_doesnt_pass("foo/Cargo.toml");
}

#[tokio::test]
async fn glob_leading_double_star() {
	let filterer = filt(&[filter("**/possum")]).await;

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

#[tokio::test]
async fn glob_trailing_double_star() {
	let filterer = filt(&[filter("possum/**")]).await;

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

#[tokio::test]
async fn glob_middle_double_star() {
	let filterer = filt(&[filter("apples/**/oranges")]).await;

	filterer.dir_doesnt_pass("/a/folder");
	filterer.file_does_pass("apples/carrots/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.dir_does_pass("apples/carrots/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");

	// different from globset/v1 behaviour, but correct:
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples/oranges/bananas");
}

#[tokio::test]
async fn glob_double_star_trailing_slash() {
	let filterer = filt(&[filter("apples/**/oranges/")]).await;

	filterer.dir_doesnt_pass("/a/folder");
	filterer.file_doesnt_pass("apples/carrots/oranges");
	filterer.file_doesnt_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_doesnt_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.dir_does_pass("apples/carrots/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.unk_doesnt_pass("apples/carrots/oranges");
	filterer.unk_doesnt_pass("apples/carrots/cauliflowers/oranges");
	filterer.unk_doesnt_pass("apples/carrots/cauliflowers/artichokes/oranges");

	// different from globset/v1 behaviour, but correct:
	filterer.file_does_pass("apples/oranges/bananas");
	filterer.dir_does_pass("apples/oranges/bananas");
}

#[tokio::test]
async fn ignore_exact_filename() {
	let filterer = filt(&[not_filter("Cargo.toml")]).await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("/test/foo/bar/Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.dir_doesnt_pass("/test/Cargo.toml");
}

#[tokio::test]
async fn ignore_exact_filenames_multiple() {
	let filterer = filt(&[not_filter("Cargo.toml"), not_filter("package.json")]).await;

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

#[tokio::test]
async fn ignore_glob_single_final_ext_star() {
	let filterer = filt(&[not_filter("Cargo.*")]).await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.dir_doesnt_pass("Cargo.toml");
}

#[tokio::test]
async fn ignore_glob_star_trailing_slash() {
	let filterer = filt(&[not_filter("Cargo.*/")]).await;

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.dir_doesnt_pass("Cargo.toml");
	filterer.unk_does_pass("Cargo.toml");
}

#[tokio::test]
async fn ignore_glob_star_leading_slash() {
	let filterer = filt(&[not_filter("/Cargo.*")]).await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.dir_doesnt_pass("Cargo.toml");
	filterer.unk_doesnt_pass("Cargo.toml");
	filterer.file_does_pass("foo/Cargo.toml");
	filterer.dir_does_pass("foo/Cargo.toml");
}

#[tokio::test]
async fn ignore_glob_leading_double_star() {
	let filterer = filt(&[not_filter("**/possum")]).await;

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

#[tokio::test]
async fn ignore_glob_trailing_double_star() {
	let filterer = filt(&[not_filter("possum/**")]).await;

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

#[tokio::test]
async fn ignore_glob_middle_double_star() {
	let filterer = filt(&[not_filter("apples/**/oranges")]).await;

	filterer.dir_does_pass("/a/folder");
	filterer.file_doesnt_pass("apples/carrots/oranges");
	filterer.file_doesnt_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_doesnt_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.dir_doesnt_pass("apples/carrots/oranges");
	filterer.dir_doesnt_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_doesnt_pass("apples/carrots/cauliflowers/artichokes/oranges");

	// different from globset/v1 behaviour, but correct:
	filterer.file_doesnt_pass("apples/oranges/bananas");
	filterer.dir_doesnt_pass("apples/oranges/bananas");
}

#[tokio::test]
async fn ignore_glob_double_star_trailing_slash() {
	let filterer = filt(&[not_filter("apples/**/oranges/")]).await;

	filterer.dir_does_pass("/a/folder");
	filterer.file_does_pass("apples/carrots/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.file_does_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.dir_doesnt_pass("apples/carrots/oranges");
	filterer.dir_doesnt_pass("apples/carrots/cauliflowers/oranges");
	filterer.dir_doesnt_pass("apples/carrots/cauliflowers/artichokes/oranges");
	filterer.unk_does_pass("apples/carrots/oranges");
	filterer.unk_does_pass("apples/carrots/cauliflowers/oranges");
	filterer.unk_does_pass("apples/carrots/cauliflowers/artichokes/oranges");

	// different from globset/v1 behaviour, but correct:
	filterer.file_doesnt_pass("apples/oranges/bananas");
	filterer.dir_doesnt_pass("apples/oranges/bananas");
}

#[tokio::test]
async fn ignores_take_precedence() {
	let filterer = filt(&[
		filter("*.docx"),
		filter("*.toml"),
		filter("*.json"),
		not_filter("*.toml"),
		not_filter("*.json"),
	])
	.await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("/test/foo/bar/Cargo.toml");
	filterer.file_doesnt_pass("package.json");
	filterer.file_doesnt_pass("/test/foo/bar/package.json");
	filterer.dir_doesnt_pass("/test/Cargo.toml");
	filterer.dir_doesnt_pass("/test/package.json");
	filterer.file_does_pass("FINAL-FINAL.docx");
}

#[tokio::test]
async fn scopes_global() {
	let filterer = filt(&[not_filter("*.toml")]).await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.dir_doesnt_pass("Cargo.toml");

	filterer.file_doesnt_pass("/outside/Cargo.toml");
	filterer.dir_doesnt_pass("/outside/Cargo.toml");

	filterer.file_does_pass("/outside/package.json");
	filterer.dir_does_pass("/outside/package.json");

	filterer.file_does_pass("package.json");
	filterer.file_does_pass("FINAL-FINAL.docx");
}

#[tokio::test]
async fn scopes_local() {
	let filterer = filt(&[not_filter("*.toml").in_path()]).await;

	filterer.file_doesnt_pass("/test/Cargo.toml");
	filterer.dir_doesnt_pass("/test/Cargo.toml");

	filterer.file_does_pass("/outside/Cargo.toml");
	filterer.dir_does_pass("/outside/Cargo.toml");

	filterer.file_does_pass("/outside/package.json");
	filterer.dir_does_pass("/outside/package.json");

	filterer.file_does_pass("package.json");
	filterer.file_does_pass("FINAL-FINAL.docx");
}

#[tokio::test]
async fn scopes_sublocal() {
	let filterer = filt(&[not_filter("*.toml").in_subpath("src")]).await;

	filterer.file_doesnt_pass("/test/src/Cargo.toml");
	filterer.dir_doesnt_pass("/test/src/Cargo.toml");

	filterer.file_does_pass("/test/Cargo.toml");
	filterer.dir_does_pass("/test/Cargo.toml");
	filterer.file_does_pass("/test/tests/Cargo.toml");
	filterer.dir_does_pass("/test/tests/Cargo.toml");

	filterer.file_does_pass("/outside/Cargo.toml");
	filterer.dir_does_pass("/outside/Cargo.toml");

	filterer.file_does_pass("/outside/package.json");
	filterer.dir_does_pass("/outside/package.json");

	filterer.file_does_pass("package.json");
	filterer.file_does_pass("FINAL-FINAL.docx");
}

// The following tests check that the "buggy"/"confusing" watchexec v1 behaviour
// is no longer present.

fn watchexec_v1_confusing_suite(filterer: Arc<TaggedFilterer>) {
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

	filterer.dir_doesnt_pass("prunes/carrots/cauliflowers/oranges");
	filterer.dir_doesnt_pass("prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_doesnt_pass("prunes/carrots/cauliflowers/oranges");
	filterer.file_doesnt_pass("prunes/carrots/cauliflowers/artichokes/oranges");
	filterer.file_doesnt_pass("prunes/oranges/bananas");
}

#[tokio::test]
async fn ignore_folder_with_bare_match() {
	let filterer = filt(&[not_filter("prunes").in_path()]).await;

	filterer.file_doesnt_pass("prunes");
	filterer.dir_doesnt_pass("prunes");
	watchexec_v1_confusing_suite(filterer);
}

#[tokio::test]
async fn ignore_folder_with_bare_and_leading_slash() {
	let filterer = filt(&[not_filter("/prunes").in_path()]).await;

	filterer.file_doesnt_pass("prunes");
	filterer.dir_doesnt_pass("prunes");
	watchexec_v1_confusing_suite(filterer);
}

#[tokio::test]
async fn ignore_folder_with_bare_and_trailing_slash() {
	let filterer = filt(&[not_filter("prunes/").in_path()]).await;

	filterer.file_does_pass("prunes");
	filterer.dir_doesnt_pass("prunes");
	watchexec_v1_confusing_suite(filterer);
}

#[tokio::test]
async fn ignore_folder_with_only_double_double_glob() {
	let filterer = filt(&[not_filter("**/prunes/**").in_path()]).await;

	filterer.file_does_pass("prunes");
	filterer.dir_does_pass("prunes");
	watchexec_v1_confusing_suite(filterer);
}

#[tokio::test]
async fn ignore_folder_with_double_and_double_double_globs() {
	let filterer = filt(&[
		not_filter("**/prunes").in_path(),
		not_filter("**/prunes/**").in_path(),
	])
	.await;

	filterer.file_doesnt_pass("prunes");
	filterer.dir_doesnt_pass("prunes");
	watchexec_v1_confusing_suite(filterer);
}
