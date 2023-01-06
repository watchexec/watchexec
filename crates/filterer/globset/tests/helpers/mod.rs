use std::{
	ffi::OsString,
	path::{Path, PathBuf},
};

use ignore_files::IgnoreFile;
use project_origins::ProjectType;
use watchexec::{
	error::RuntimeError,
	event::{Event, FileType, Priority, Tag},
	filter::Filterer,
};
use watchexec_filterer_globset::GlobsetFilterer;
use watchexec_filterer_ignore::IgnoreFilterer;

pub mod globset {
	pub use super::globset_filt as filt;
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

impl PathHarness for GlobsetFilterer {}
impl PathHarness for IgnoreFilterer {}

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
		filters.iter().map(|s| ((*s).to_string(), None)),
		ignores.iter().map(|s| ((*s).to_string(), None)),
		vec![],
		extensions.iter().map(OsString::from),
	)
	.await
	.expect("making filterer")
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
