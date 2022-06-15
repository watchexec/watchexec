use std::num::{NonZeroI32, NonZeroI64};

use watchexec::{
	event::{filekind::*, ProcessEnd, Source},
	signal::{process::SubSignal, source::MainSignal},
};

use watchexec_filterer_tagged::TaggedFilterer;

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

#[tokio::test]
async fn pid_set_single() {
	let f = filter("process:=1234");
	assert_eq!(f, filter("pid:=1234"));
	assert_eq!(f, filter("process=1234"));
	assert_eq!(f, filter("pid=1234"));

	let filterer = filt(&[f]).await;

	filterer.pid_does_pass(1234);
	filterer.pid_doesnt_pass(5678);
	filterer.pid_doesnt_pass(12345);
	filterer.pid_doesnt_pass(123);
}

#[tokio::test]
async fn pid_set_multiple() {
	let filterer = filt(&[filter("pid=123,456")]).await;

	filterer.pid_does_pass(123);
	filterer.pid_does_pass(456);
	filterer.pid_doesnt_pass(123456);
	filterer.pid_doesnt_pass(12);
	filterer.pid_doesnt_pass(23);
	filterer.pid_doesnt_pass(45);
	filterer.pid_doesnt_pass(56);
	filterer.pid_doesnt_pass(1234);
	filterer.pid_doesnt_pass(3456);
	filterer.pid_doesnt_pass(4567);
	filterer.pid_doesnt_pass(34567);
	filterer.pid_doesnt_pass(0);
}

#[tokio::test]
async fn pid_equals() {
	let f = filter("process==1234");
	assert_eq!(f, filter("pid==1234"));

	let filterer = filt(&[f]).await;

	filterer.pid_does_pass(1234);
	filterer.pid_doesnt_pass(5678);
	filterer.pid_doesnt_pass(12345);
	filterer.pid_doesnt_pass(123);
}

#[tokio::test]
async fn signal_set_single_without_sig() {
	let f = filter("signal=INT");
	assert_eq!(f, filter("sig=INT"));
	assert_eq!(f, filter("signal:=INT"));
	assert_eq!(f, filter("sig:=INT"));

	let filterer = filt(&[f]).await;

	filterer.signal_does_pass(MainSignal::Interrupt);
	filterer.signal_doesnt_pass(MainSignal::Hangup);
}

#[tokio::test]
async fn signal_set_single_with_sig() {
	let filterer = filt(&[filter("signal:=SIGINT")]).await;

	filterer.signal_does_pass(MainSignal::Interrupt);
	filterer.signal_doesnt_pass(MainSignal::Hangup);
}

#[tokio::test]
async fn signal_set_multiple_without_sig() {
	let filterer = filt(&[filter("sig:=INT,TERM")]).await;

	filterer.signal_does_pass(MainSignal::Interrupt);
	filterer.signal_does_pass(MainSignal::Terminate);
	filterer.signal_doesnt_pass(MainSignal::Hangup);
}

#[tokio::test]
async fn signal_set_multiple_with_sig() {
	let filterer = filt(&[filter("signal:=SIGINT,SIGTERM")]).await;

	filterer.signal_does_pass(MainSignal::Interrupt);
	filterer.signal_does_pass(MainSignal::Terminate);
	filterer.signal_doesnt_pass(MainSignal::Hangup);
}

#[tokio::test]
async fn signal_set_multiple_mixed_sig() {
	let filterer = filt(&[filter("sig:=SIGINT,TERM")]).await;

	filterer.signal_does_pass(MainSignal::Interrupt);
	filterer.signal_does_pass(MainSignal::Terminate);
	filterer.signal_doesnt_pass(MainSignal::Hangup);
}

#[tokio::test]
async fn signal_equals_without_sig() {
	let filterer = filt(&[filter("sig==INT")]).await;

	filterer.signal_does_pass(MainSignal::Interrupt);
	filterer.signal_doesnt_pass(MainSignal::Hangup);
}

#[tokio::test]
async fn signal_equals_with_sig() {
	let filterer = filt(&[filter("signal==SIGINT")]).await;

	filterer.signal_does_pass(MainSignal::Interrupt);
	filterer.signal_doesnt_pass(MainSignal::Hangup);
}

#[tokio::test]
async fn signal_set_single_numbers() {
	let filterer = filt(&[filter("signal:=2")]).await;

	filterer.signal_does_pass(MainSignal::Interrupt);
	filterer.signal_doesnt_pass(MainSignal::Hangup);
}

#[tokio::test]
async fn signal_set_multiple_numbers() {
	let filterer = filt(&[filter("sig:=2,15")]).await;

	filterer.signal_does_pass(MainSignal::Interrupt);
	filterer.signal_does_pass(MainSignal::Terminate);
	filterer.signal_doesnt_pass(MainSignal::Hangup);
}

#[tokio::test]
async fn signal_equals_numbers() {
	let filterer = filt(&[filter("sig==2")]).await;

	filterer.signal_does_pass(MainSignal::Interrupt);
	filterer.signal_doesnt_pass(MainSignal::Hangup);
}

#[tokio::test]
async fn signal_set_all_mixed() {
	let filterer = filt(&[filter("signal:=SIGHUP,INT,15")]).await;

	filterer.signal_does_pass(MainSignal::Hangup);
	filterer.signal_does_pass(MainSignal::Interrupt);
	filterer.signal_does_pass(MainSignal::Terminate);
	filterer.signal_doesnt_pass(MainSignal::User1);
}

#[tokio::test]
async fn complete_empty() {
	let f = filter("complete=_");
	assert_eq!(f, filter("complete*=_"));
	assert_eq!(f, filter("exit=_"));
	assert_eq!(f, filter("exit*=_"));

	let filterer = filt(&[f]).await;

	filterer.complete_does_pass(None);
	filterer.complete_doesnt_pass(Some(ProcessEnd::Success));
	filterer.complete_doesnt_pass(Some(ProcessEnd::ExitError(NonZeroI64::new(1).unwrap())));
}

#[tokio::test]
async fn complete_any() {
	let filterer = filt(&[filter("complete=*")]).await;

	filterer.complete_does_pass(Some(ProcessEnd::Success));
	filterer.complete_does_pass(Some(ProcessEnd::ExitError(NonZeroI64::new(1).unwrap())));
	filterer.complete_does_pass(None);
}

#[tokio::test]
async fn complete_with_success() {
	let filterer = filt(&[filter("complete*=success")]).await;

	filterer.complete_does_pass(Some(ProcessEnd::Success));
	filterer.complete_doesnt_pass(Some(ProcessEnd::ExitError(NonZeroI64::new(1).unwrap())));
	filterer.complete_doesnt_pass(None);
}

#[tokio::test]
async fn complete_with_continued() {
	let filterer = filt(&[filter("complete*=continued")]).await;

	filterer.complete_does_pass(Some(ProcessEnd::Continued));
	filterer.complete_doesnt_pass(Some(ProcessEnd::Success));
	filterer.complete_doesnt_pass(Some(ProcessEnd::ExitError(NonZeroI64::new(1).unwrap())));
	filterer.complete_doesnt_pass(None);
}

#[tokio::test]
async fn complete_with_specific_exit_error() {
	let filterer = filt(&[filter("complete*=error(1)")]).await;

	filterer.complete_does_pass(Some(ProcessEnd::ExitError(NonZeroI64::new(1).unwrap())));
	filterer.complete_doesnt_pass(Some(ProcessEnd::Success));
	filterer.complete_doesnt_pass(None);
}

#[tokio::test]
async fn complete_with_any_exit_error() {
	let filterer = filt(&[filter("complete*=error(*)")]).await;

	filterer.complete_does_pass(Some(ProcessEnd::ExitError(NonZeroI64::new(1).unwrap())));
	filterer.complete_does_pass(Some(ProcessEnd::ExitError(NonZeroI64::new(63).unwrap())));
	filterer.complete_does_pass(Some(ProcessEnd::ExitError(
		NonZeroI64::new(-12823912738).unwrap(),
	)));
	filterer.complete_doesnt_pass(Some(ProcessEnd::ExitStop(NonZeroI32::new(63).unwrap())));
	filterer.complete_doesnt_pass(Some(ProcessEnd::Success));
	filterer.complete_doesnt_pass(None);
}

#[tokio::test]
async fn complete_with_specific_stop() {
	let filterer = filt(&[filter("complete*=stop(19)")]).await;

	filterer.complete_does_pass(Some(ProcessEnd::ExitStop(NonZeroI32::new(19).unwrap())));
	filterer.complete_doesnt_pass(Some(ProcessEnd::Success));
	filterer.complete_doesnt_pass(None);
}

#[tokio::test]
async fn complete_with_any_stop() {
	let filterer = filt(&[filter("complete*=stop(*)")]).await;

	filterer.complete_does_pass(Some(ProcessEnd::ExitStop(NonZeroI32::new(1).unwrap())));
	filterer.complete_does_pass(Some(ProcessEnd::ExitStop(NonZeroI32::new(63).unwrap())));
	filterer.complete_does_pass(Some(ProcessEnd::ExitStop(
		NonZeroI32::new(-128239127).unwrap(),
	)));
	filterer.complete_doesnt_pass(Some(ProcessEnd::ExitError(NonZeroI64::new(63).unwrap())));
	filterer.complete_doesnt_pass(Some(ProcessEnd::Success));
	filterer.complete_doesnt_pass(None);
}

#[tokio::test]
async fn complete_with_specific_exception() {
	let filterer = filt(&[filter("complete*=exception(4B53)")]).await;

	filterer.complete_does_pass(Some(ProcessEnd::Exception(NonZeroI32::new(19283).unwrap())));
	filterer.complete_doesnt_pass(Some(ProcessEnd::Success));
	filterer.complete_doesnt_pass(None);
}

#[tokio::test]
async fn complete_with_any_exception() {
	let filterer = filt(&[filter("complete*=exception(*)")]).await;

	filterer.complete_does_pass(Some(ProcessEnd::Exception(NonZeroI32::new(1).unwrap())));
	filterer.complete_does_pass(Some(ProcessEnd::Exception(NonZeroI32::new(63).unwrap())));
	filterer.complete_does_pass(Some(ProcessEnd::Exception(
		NonZeroI32::new(-128239127).unwrap(),
	)));
	filterer.complete_doesnt_pass(Some(ProcessEnd::ExitStop(NonZeroI32::new(63).unwrap())));
	filterer.complete_doesnt_pass(Some(ProcessEnd::ExitError(NonZeroI64::new(63).unwrap())));
	filterer.complete_doesnt_pass(Some(ProcessEnd::Success));
	filterer.complete_doesnt_pass(None);
}

#[tokio::test]
async fn complete_with_specific_signal_with_sig() {
	let filterer = filt(&[filter("complete*=signal(SIGINT)")]).await;

	filterer.complete_does_pass(Some(ProcessEnd::ExitSignal(SubSignal::Interrupt)));
	filterer.complete_doesnt_pass(Some(ProcessEnd::ExitStop(NonZeroI32::new(19).unwrap())));
	filterer.complete_doesnt_pass(Some(ProcessEnd::Success));
	filterer.complete_doesnt_pass(None);
}

#[tokio::test]
async fn complete_with_specific_signal_without_sig() {
	let filterer = filt(&[filter("complete*=signal(INT)")]).await;

	filterer.complete_does_pass(Some(ProcessEnd::ExitSignal(SubSignal::Interrupt)));
	filterer.complete_doesnt_pass(Some(ProcessEnd::ExitStop(NonZeroI32::new(19).unwrap())));
	filterer.complete_doesnt_pass(Some(ProcessEnd::Success));
	filterer.complete_doesnt_pass(None);
}

#[tokio::test]
async fn complete_with_specific_signal_number() {
	let filterer = filt(&[filter("complete*=signal(2)")]).await;

	filterer.complete_does_pass(Some(ProcessEnd::ExitSignal(SubSignal::Interrupt)));
	filterer.complete_doesnt_pass(Some(ProcessEnd::ExitStop(NonZeroI32::new(19).unwrap())));
	filterer.complete_doesnt_pass(Some(ProcessEnd::Success));
	filterer.complete_doesnt_pass(None);
}

#[tokio::test]
async fn complete_with_any_signal() {
	let filterer = filt(&[filter("complete*=signal(*)")]).await;

	filterer.complete_does_pass(Some(ProcessEnd::ExitSignal(SubSignal::Interrupt)));
	filterer.complete_does_pass(Some(ProcessEnd::ExitSignal(SubSignal::Terminate)));
	filterer.complete_does_pass(Some(ProcessEnd::ExitSignal(SubSignal::Custom(123))));
	filterer.complete_doesnt_pass(Some(ProcessEnd::ExitStop(NonZeroI32::new(63).unwrap())));
	filterer.complete_doesnt_pass(Some(ProcessEnd::ExitError(NonZeroI64::new(63).unwrap())));
	filterer.complete_doesnt_pass(Some(ProcessEnd::Success));
	filterer.complete_doesnt_pass(None);
}

#[tokio::test]
async fn priority_auto() {
	let filterer = filt(&[filter("priority=normal")]).await;

	filterer.priority_doesnt_pass(Priority::Low);
	filterer.priority_does_pass(Priority::Normal);
	filterer.priority_doesnt_pass(Priority::High);
}

#[tokio::test]
async fn priority_set() {
	let filterer = filt(&[filter("priority:=normal,high")]).await;

	filterer.priority_doesnt_pass(Priority::Low);
	filterer.priority_does_pass(Priority::Normal);
	filterer.priority_does_pass(Priority::High);
}

#[tokio::test]
async fn priority_none() {
	let filterer = filt(&[]).await;

	filterer.priority_does_pass(Priority::Low);
	filterer.priority_does_pass(Priority::Normal);
	filterer.priority_does_pass(Priority::High);
}
