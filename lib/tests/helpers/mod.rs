#![allow(dead_code)]

use std::{
	ffi::OsString,
	path::{Path, PathBuf},
	str::FromStr,
	sync::Arc,
};

use watchexec::{
	error::RuntimeError,
	event::{filekind::FileEventKind, Event, FileType, Priority, ProcessEnd, Source, Tag},
	filter::{
		globset::GlobsetFilterer,
		tagged::{files::FilterFile, Filter, Matcher, Op, Pattern, TaggedFilterer},
		Filterer,
	},
	ignore::{IgnoreFile, IgnoreFilterer},
	project::ProjectType,
	signal::source::MainSignal,
};

pub mod ignore {
	pub use super::ig_file as file;
	pub use super::ignore_filt as filt;
	pub use super::Applies;
	pub use super::PathHarness;
	pub use watchexec::event::Priority;
}

pub mod globset {
	pub use super::globset_filt as filt;
	pub use super::ig_file as file;
	pub use super::Applies;
	pub use super::PathHarness;
	pub use watchexec::event::Priority;
}

pub mod tagged {
	pub use super::ig_file as file;
	pub use super::tagged_filt as filt;
	pub use super::Applies;
	pub use super::FilterExt;
	pub use super::PathHarness;
	pub use super::TaggedHarness;
	pub use super::{filter, glob_filter, notglob_filter};
	pub use watchexec::event::Priority;
}

pub mod tagged_ff {
	pub use super::ff_file as file;
	pub use super::tagged::*;
	pub use super::tagged_fffilt as filt;
}

pub trait PathHarness: Filterer {
	fn check_path(
		&self,
		path: PathBuf,
		file_type: Option<FileType>,
	) -> std::result::Result<bool, RuntimeError> {
		let event = Event {
			tags: vec![Tag::Path { path, file_type }],
			metadata: Default::default(),
		};

		self.check_event(&event, Priority::Normal)
	}

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

impl PathHarness for GlobsetFilterer {}
impl PathHarness for TaggedFilterer {}
impl PathHarness for IgnoreFilterer {}

pub trait TaggedHarness {
	fn check_tag(&self, tag: Tag, priority: Priority) -> std::result::Result<bool, RuntimeError>;

	fn priority_pass(&self, priority: Priority, pass: bool) {
		tracing::info!(?priority, ?pass, "check");

		assert_eq!(
			self.check_tag(Tag::Source(Source::Filesystem), priority)
				.unwrap(),
			pass,
			"{priority:?} (expected {})",
			if pass { "pass" } else { "fail" }
		);
	}

	fn priority_does_pass(&self, priority: Priority) {
		self.priority_pass(priority, true);
	}

	fn priority_doesnt_pass(&self, priority: Priority) {
		self.priority_pass(priority, false);
	}

	fn tag_pass(&self, tag: Tag, pass: bool) {
		tracing::info!(?tag, ?pass, "check");

		assert_eq!(
			self.check_tag(tag.clone(), Priority::Normal).unwrap(),
			pass,
			"{tag:?} (expected {})",
			if pass { "pass" } else { "fail" }
		);
	}

	fn fek_does_pass(&self, fek: FileEventKind) {
		self.tag_pass(Tag::FileEventKind(fek), true);
	}

	fn fek_doesnt_pass(&self, fek: FileEventKind) {
		self.tag_pass(Tag::FileEventKind(fek), false);
	}

	fn source_does_pass(&self, source: Source) {
		self.tag_pass(Tag::Source(source), true);
	}

	fn source_doesnt_pass(&self, source: Source) {
		self.tag_pass(Tag::Source(source), false);
	}

	fn pid_does_pass(&self, pid: u32) {
		self.tag_pass(Tag::Process(pid), true);
	}

	fn pid_doesnt_pass(&self, pid: u32) {
		self.tag_pass(Tag::Process(pid), false);
	}

	fn signal_does_pass(&self, sig: MainSignal) {
		self.tag_pass(Tag::Signal(sig), true);
	}

	fn signal_doesnt_pass(&self, sig: MainSignal) {
		self.tag_pass(Tag::Signal(sig), false);
	}

	fn complete_does_pass(&self, exit: Option<ProcessEnd>) {
		self.tag_pass(Tag::ProcessCompletion(exit), true);
	}

	fn complete_doesnt_pass(&self, exit: Option<ProcessEnd>) {
		self.tag_pass(Tag::ProcessCompletion(exit), false);
	}
}

impl TaggedHarness for TaggedFilterer {
	fn check_tag(&self, tag: Tag, priority: Priority) -> std::result::Result<bool, RuntimeError> {
		let event = Event {
			tags: vec![tag],
			metadata: Default::default(),
		};

		self.check_event(&event, priority)
	}
}

fn tracing_init() {
	use tracing_subscriber::{
		fmt::{format::FmtSpan, Subscriber},
		util::SubscriberInitExt,
		EnvFilter,
	};
	Subscriber::builder()
		.pretty()
		.with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
		.with_env_filter(EnvFilter::from_default_env())
		.finish()
		.try_init()
		.ok();
}

pub async fn globset_filt(
	filters: &[&str],
	ignores: &[&str],
	extensions: &[&str],
) -> GlobsetFilterer {
	let origin = dunce::canonicalize(".").unwrap();
	tracing_init();
	GlobsetFilterer::new(
		origin,
		filters.iter().map(|s| (s.to_string(), None)),
		ignores.iter().map(|s| (s.to_string(), None)),
		vec![],
		extensions.iter().map(OsString::from),
	)
	.await
	.expect("making filterer")
}

pub async fn ignore_filt(origin: &str, ignore_files: &[IgnoreFile]) -> IgnoreFilterer {
	tracing_init();
	let origin = dunce::canonicalize(".").unwrap().join(origin);
	IgnoreFilterer::new(origin, ignore_files)
		.await
		.expect("making filterer")
}

pub async fn tagged_filt(filters: &[Filter]) -> Arc<TaggedFilterer> {
	let origin = dunce::canonicalize(".").unwrap();
	tracing_init();
	let filterer = TaggedFilterer::new(origin.clone(), origin).expect("creating filterer");
	filterer.add_filters(filters).await.expect("adding filters");
	filterer
}

pub async fn tagged_igfilt(origin: &str, ignore_files: &[IgnoreFile]) -> Arc<TaggedFilterer> {
	let origin = dunce::canonicalize(".").unwrap().join(origin);
	tracing_init();
	let filterer = TaggedFilterer::new(origin.clone(), origin).expect("creating filterer");
	for file in ignore_files {
		tracing::info!(?file, "loading ignore file");
		filterer
			.add_ignore_file(file)
			.await
			.expect("adding ignore file");
	}
	filterer
}

pub async fn tagged_fffilt(
	origin: &str,
	ignore_files: &[IgnoreFile],
	filter_files: &[FilterFile],
) -> Arc<TaggedFilterer> {
	let filterer = tagged_igfilt(origin, ignore_files).await;
	let mut filters = Vec::new();
	for file in filter_files {
		tracing::info!(?file, "loading filter file");
		filters.extend(file.load().await.expect("loading filter file"));
	}

	filterer
		.add_filters(&filters)
		.await
		.expect("adding filters");

	filterer
}

pub fn ig_file(name: &str) -> IgnoreFile {
	let path = dunce::canonicalize(".")
		.unwrap()
		.join("tests")
		.join("ignores")
		.join(name);
	IgnoreFile {
		path,
		applies_in: None,
		applies_to: None,
	}
}

pub fn ff_file(name: &str) -> FilterFile {
	FilterFile(ig_file(name))
}

pub trait Applies {
	fn applies_in(self, origin: &str) -> Self;
	fn applies_to(self, project_type: ProjectType) -> Self;
}

impl Applies for IgnoreFile {
	fn applies_in(mut self, origin: &str) -> Self {
		let origin = dunce::canonicalize(".").unwrap().join(origin);
		self.applies_in = Some(origin);
		self
	}

	fn applies_to(mut self, project_type: ProjectType) -> Self {
		self.applies_to = Some(project_type);
		self
	}
}

impl Applies for FilterFile {
	fn applies_in(self, origin: &str) -> Self {
		Self(self.0.applies_in(origin))
	}

	fn applies_to(self, project_type: ProjectType) -> Self {
		Self(self.0.applies_to(project_type))
	}
}

pub fn filter(expr: &str) -> Filter {
	Filter::from_str(expr).expect("parse filter")
}

pub fn glob_filter(pat: &str) -> Filter {
	Filter {
		in_path: None,
		on: Matcher::Path,
		op: Op::Glob,
		pat: Pattern::Glob(pat.into()),
		negate: false,
	}
}

pub fn notglob_filter(pat: &str) -> Filter {
	Filter {
		in_path: None,
		on: Matcher::Path,
		op: Op::NotGlob,
		pat: Pattern::Glob(pat.into()),
		negate: false,
	}
}

pub trait FilterExt {
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
