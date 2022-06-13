use std::path::{Path, PathBuf};

use project_origins::ProjectType;
use watchexec::{
	error::RuntimeError,
	event::{filekind::FileEventKind, Event, FileType, Priority, ProcessEnd, Source, Tag},
	filter::Filterer,
	ignore::{IgnoreFile, IgnoreFilterer},
	signal::source::MainSignal,
};

pub mod ignore {
	pub use super::ig_file as file;
	pub use super::ignore_filt as filt;
	pub use super::Applies;
	pub use super::PathHarness;
	pub use watchexec::event::Priority;
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

pub async fn ignore_filt(origin: &str, ignore_files: &[IgnoreFile]) -> IgnoreFilterer {
	tracing_init();
	let origin = dunce::canonicalize(".").unwrap().join(origin);
	IgnoreFilterer::new(origin, ignore_files)
		.await
		.expect("making filterer")
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
