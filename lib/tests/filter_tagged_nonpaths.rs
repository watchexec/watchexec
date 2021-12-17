use watchexec::{
	event::{filekind::*, ProcessEnd, Source},
	filter::tagged::TaggedFilterer,
	signal::source::MainSignal,
};

mod helpers;
use helpers::tagged::*;

#[tokio::test]
async fn empty_filter_passes_everything() {
	let filterer = filt(&[]).await;

	filterer.source_does_pass(Source::Keyboard);
	filterer.fek_does_pass(FileEventKind::Create(CreateKind::File));
	filterer.pid_does_pass(1234);
	filterer.signal_does_pass(MainSignal::User1);
	filterer.complete_does_pass(None);
	filterer.complete_does_pass(Some(ProcessEnd::Success));
}

// Source is used as a relatively simple test case for common text-based ops, so
// these aren't repeated for the other tags, which instead focus on their own
// special characteristics.

#[tokio::test]
async fn source_exact() {
	let filterer = filt(&[filter("source==keyboard")]).await;

	filterer.source_does_pass(Source::Keyboard);
	filterer.source_doesnt_pass(Source::Mouse);
}

#[tokio::test]
async fn source_glob() {
	let filterer = filt(&[filter("source*=*i*m*")]).await;

	filterer.source_does_pass(Source::Filesystem);
	filterer.source_does_pass(Source::Time);
	filterer.source_doesnt_pass(Source::Internal);
}

#[tokio::test]
async fn source_regex() {
	let filterer = filt(&[filter("source~=(keyboard|mouse)")]).await;

	filterer.source_does_pass(Source::Keyboard);
	filterer.source_does_pass(Source::Mouse);
	filterer.source_doesnt_pass(Source::Internal);
}

#[tokio::test]
async fn source_two_filters() {
	let filterer = filt(&[filter("source*=*s*"), filter("source!=mouse")]).await;

	filterer.source_doesnt_pass(Source::Mouse);
	filterer.source_does_pass(Source::Filesystem);
}

#[tokio::test]
async fn source_allowlisting() {
	// allowlisting is vastly easier to achieve with e.g. `source==mouse`
	// but this pattern is nonetheless useful for more complex cases.
	let filterer = filt(&[filter("source*!*"), filter("!source==mouse")]).await;

	filterer.source_does_pass(Source::Mouse);
	filterer.source_doesnt_pass(Source::Filesystem);
}

#[tokio::test]
async fn source_set() {
	let f = filter("source:=keyboard,mouse");
	assert_eq!(f, filter("source=keyboard,mouse"));

	let filterer = filt(&[f]).await;
	filterer.source_does_pass(Source::Keyboard);
	filterer.source_does_pass(Source::Mouse);
	filterer.source_doesnt_pass(Source::Internal);

	let filterer = filt(&[filter("source:!keyboard,mouse")]).await;
	filterer.source_doesnt_pass(Source::Keyboard);
	filterer.source_doesnt_pass(Source::Mouse);
	filterer.source_does_pass(Source::Internal);
}

#[tokio::test]
async fn fek_glob_level_one() {
	let f = filter("kind*=Create(*)");
	assert_eq!(f, filter("fek*=Create(*)"));
	assert_eq!(f, filter("kind=Create(*)"));
	assert_eq!(f, filter("fek=Create(*)"));

	let filterer = filt(&[f]).await;

	filterer.fek_does_pass(FileEventKind::Create(CreateKind::Any));
	filterer.fek_does_pass(FileEventKind::Create(CreateKind::File));
	filterer.fek_doesnt_pass(FileEventKind::Modify(ModifyKind::Data(DataChange::Content)));
}

#[tokio::test]
async fn fek_glob_level_two() {
	let filterer = filt(&[filter("fek=Modify(Data(*))")]).await;

	filterer.fek_does_pass(FileEventKind::Modify(ModifyKind::Data(DataChange::Content)));
	filterer.fek_doesnt_pass(FileEventKind::Modify(ModifyKind::Other));
	filterer.fek_doesnt_pass(FileEventKind::Modify(ModifyKind::Metadata(
		MetadataKind::Permissions,
	)));
	filterer.fek_doesnt_pass(FileEventKind::Create(CreateKind::Any));
}

#[tokio::test]
async fn fek_level_three() {
	fn suite(filterer: &TaggedFilterer) {
		filterer.fek_does_pass(FileEventKind::Modify(ModifyKind::Data(DataChange::Content)));
		filterer.fek_doesnt_pass(FileEventKind::Modify(ModifyKind::Data(DataChange::Size)));
		filterer.fek_doesnt_pass(FileEventKind::Modify(ModifyKind::Other));
		filterer.fek_doesnt_pass(FileEventKind::Modify(ModifyKind::Metadata(
			MetadataKind::Permissions,
		)));
		filterer.fek_doesnt_pass(FileEventKind::Create(CreateKind::Any));
	}

	suite(filt(&[filter("fek=Modify(Data(Content))")]).await.as_ref());
	suite(filt(&[filter("fek==Modify(Data(Content))")]).await.as_ref());
}

