use std::sync::Arc;

use watchexec_filterer_tagged::TaggedFilterer;

mod helpers;
use helpers::tagged::*;

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
	let filterer = filt(&[glob_filter("Cargo.toml")]).await;

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
	let filterer = filt(&[glob_filter("Cargo.toml"), glob_filter("package.json")]).await;

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
	let filterer = filt(&[glob_filter("Cargo.*")]).await;

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.file_doesnt_pass("Gemfile.toml");
	filterer.file_doesnt_pass("FINAL-FINAL.docx");
	filterer.dir_doesnt_pass("/a/folder");
	filterer.dir_does_pass("Cargo.toml");
}

#[tokio::test]
async fn glob_star_trailing_slash() {
	let filterer = filt(&[glob_filter("Cargo.*/")]).await;

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
	let filterer = filt(&[glob_filter("/Cargo.*")]).await;

	filterer.file_does_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");
	filterer.dir_does_pass("Cargo.toml");
	filterer.unk_does_pass("Cargo.toml");
	filterer.file_doesnt_pass("foo/Cargo.toml");
	filterer.dir_doesnt_pass("foo/Cargo.toml");
}

#[tokio::test]
async fn glob_leading_double_star() {
	let filterer = filt(&[glob_filter("**/possum")]).await;

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
	let filterer = filt(&[glob_filter("possum/**")]).await;

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
	let filterer = filt(&[glob_filter("apples/**/oranges")]).await;

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
	let filterer = filt(&[glob_filter("apples/**/oranges/")]).await;

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
	let filterer = filt(&[notglob_filter("Cargo.toml")]).await;

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
	let filterer = filt(&[notglob_filter("Cargo.toml"), notglob_filter("package.json")]).await;

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
	let filterer = filt(&[notglob_filter("Cargo.*")]).await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.file_does_pass("Gemfile.toml");
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.dir_doesnt_pass("Cargo.toml");
}

#[tokio::test]
async fn ignore_glob_star_trailing_slash() {
	let filterer = filt(&[notglob_filter("Cargo.*/")]).await;

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
	let filterer = filt(&[notglob_filter("/Cargo.*")]).await;

	filterer.file_doesnt_pass("Cargo.toml");
	filterer.file_doesnt_pass("Cargo.json");
	filterer.dir_doesnt_pass("Cargo.toml");
	filterer.unk_doesnt_pass("Cargo.toml");
	filterer.file_does_pass("foo/Cargo.toml");
	filterer.dir_does_pass("foo/Cargo.toml");
}

#[tokio::test]
async fn ignore_glob_leading_double_star() {
	let filterer = filt(&[notglob_filter("**/possum")]).await;

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
	let filterer = filt(&[notglob_filter("possum/**")]).await;

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
	let filterer = filt(&[notglob_filter("apples/**/oranges")]).await;

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
	let filterer = filt(&[notglob_filter("apples/**/oranges/")]).await;

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
		glob_filter("*.docx"),
		glob_filter("*.toml"),
		glob_filter("*.json"),
		notglob_filter("*.toml"),
		notglob_filter("*.json"),
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
	let filterer = filt(&[notglob_filter("*.toml")]).await;

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
	let filterer = filt(&[notglob_filter("*.toml").in_path()]).await;

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
	let filterer = filt(&[notglob_filter("*.toml").in_subpath("src")]).await;

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
	let filterer = filt(&[notglob_filter("prunes").in_path()]).await;

	filterer.file_doesnt_pass("prunes");
	filterer.dir_doesnt_pass("prunes");
	watchexec_v1_confusing_suite(filterer);
}

#[tokio::test]
async fn ignore_folder_with_bare_and_leading_slash() {
	let filterer = filt(&[notglob_filter("/prunes").in_path()]).await;

	filterer.file_doesnt_pass("prunes");
	filterer.dir_doesnt_pass("prunes");
	watchexec_v1_confusing_suite(filterer);
}

#[tokio::test]
async fn ignore_folder_with_bare_and_trailing_slash() {
	let filterer = filt(&[notglob_filter("prunes/").in_path()]).await;

	filterer.file_does_pass("prunes");
	filterer.dir_doesnt_pass("prunes");
	watchexec_v1_confusing_suite(filterer);
}

#[tokio::test]
async fn ignore_folder_with_only_double_double_glob() {
	let filterer = filt(&[notglob_filter("**/prunes/**").in_path()]).await;

	filterer.file_does_pass("prunes");
	filterer.dir_does_pass("prunes");
	watchexec_v1_confusing_suite(filterer);
}

#[tokio::test]
async fn ignore_folder_with_double_and_double_double_globs() {
	let filterer = filt(&[
		notglob_filter("**/prunes").in_path(),
		notglob_filter("**/prunes/**").in_path(),
	])
	.await;

	filterer.file_doesnt_pass("prunes");
	filterer.dir_doesnt_pass("prunes");
	watchexec_v1_confusing_suite(filterer);
}
