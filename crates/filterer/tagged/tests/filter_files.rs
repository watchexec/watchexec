use watchexec::{
	event::{filekind::*, ProcessEnd, Source},
	signal::source::MainSignal,
};

mod helpers;
use helpers::tagged_ff::*;

#[tokio::test]
async fn empty_filter_passes_everything() {
	let filterer = filt("", &[], &[file("empty.wef").await]).await;

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

	filterer.source_does_pass(Source::Keyboard);
	filterer.fek_does_pass(FileEventKind::Create(CreateKind::File));
	filterer.pid_does_pass(1234);
	filterer.signal_does_pass(MainSignal::User1);
	filterer.complete_does_pass(None);
	filterer.complete_does_pass(Some(ProcessEnd::Success));
}

#[tokio::test]
async fn folder() {
	let filterer = filt("", &[], &[file("folder.wef").await]).await;

	filterer.file_doesnt_pass("apples");
	filterer.file_doesnt_pass("apples/oranges/bananas");
	filterer.dir_doesnt_pass("apples");
	filterer.dir_doesnt_pass("apples/carrots");

	filterer.file_doesnt_pass("raw-prunes");
	filterer.dir_doesnt_pass("raw-prunes");

	filterer.file_doesnt_pass("prunes");
	filterer.file_doesnt_pass("prunes/oranges/bananas");

	filterer.dir_does_pass("prunes");
	filterer.dir_does_pass("prunes/carrots/cauliflowers/oranges");
}

#[tokio::test]
async fn patterns() {
	let filterer = filt("", &[], &[file("path-patterns.wef").await]).await;

	// Unmatched
	filterer.file_does_pass("FINAL-FINAL.docx");
	filterer.dir_does_pass("/a/folder");
	filterer.file_does_pass("rat");
	filterer.file_does_pass("foo/bar/rat");
	filterer.file_does_pass("/foo/bar/rat");

	// Cargo.toml
	filterer.file_doesnt_pass("Cargo.toml");
	filterer.dir_doesnt_pass("Cargo.toml");
	filterer.file_does_pass("Cargo.json");

	// package.json
	filterer.file_doesnt_pass("package.json");
	filterer.dir_doesnt_pass("package.json");
	filterer.file_does_pass("package.toml");

	// *.gemspec
	filterer.file_doesnt_pass("pearl.gemspec");
	filterer.dir_doesnt_pass("sapphire.gemspec");
	filterer.file_doesnt_pass(".gemspec");
	filterer.file_does_pass("diamond.gemspecial");

	// test-[^u]+
	filterer.file_does_pass("test-unit");
	filterer.dir_doesnt_pass("test-integration");
	filterer.file_does_pass("tester-helper");

	// [.]sw[a-z]$
	filterer.file_doesnt_pass("source.swa");
	filterer.file_doesnt_pass(".source.swb");
	filterer.file_doesnt_pass("sub/source.swc");
	filterer.file_does_pass("sub/dir.swa/file");
	filterer.file_does_pass("source.sw1");
}

#[tokio::test]
async fn negate() {
	let filterer = filt("", &[], &[file("negate.wef").await]).await;

	filterer.file_doesnt_pass("yeah");
	filterer.file_does_pass("nah");
	filterer.file_does_pass("nah.yeah");
}

#[tokio::test]
async fn ignores_and_filters() {
	let filterer = filt("", &[file("globs").await.0], &[file("folder.wef").await]).await;

	// ignored
	filterer.dir_doesnt_pass("test-helper");

	// not filtered
	filterer.dir_doesnt_pass("tester-helper");

	// not ignored && filtered
	filterer.dir_does_pass("prunes/tester-helper");
}
