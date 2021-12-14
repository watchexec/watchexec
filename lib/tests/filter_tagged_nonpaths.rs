use watchexec::{
	event::{filekind::*, ProcessEnd, Source},
	signal::source::MainSignal,
};

mod helpers;
use helpers::tagged::*;

#[tokio::test]
async fn empty_filter_passes_everything() {
	let filterer = filt(&[]).await;

	filterer.fek_does_pass(FileEventKind::Create(CreateKind::File));
	filterer.source_does_pass(Source::Keyboard);
	filterer.pid_does_pass(1234);
	filterer.signal_does_pass(MainSignal::User1);
	filterer.complete_does_pass(None);
	filterer.complete_does_pass(Some(ProcessEnd::Success));
}

#[tokio::test]
async fn source_exact() {
	let filterer = filt(&[filter("source=keyboard")]).await;

	filterer.source_does_pass(Source::Keyboard);
	filterer.source_doesnt_pass(Source::Mouse);
}
