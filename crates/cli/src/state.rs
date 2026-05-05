use std::{
	env::var_os,
	io::Write,
	path::PathBuf,
	process::ExitCode,
	sync::{Arc, Mutex, OnceLock},
};

use watchexec::Watchexec;

use miette::{IntoDiagnostic, Result};
use tempfile::NamedTempFile;

use crate::{
	args::Args,
	socket::{SocketSet, Sockets},
};

pub type State = Arc<InnerState>;

pub async fn new(args: &Args) -> Result<State> {
	let socket_set = if args.command.socket.is_empty() {
		None
	} else {
		let mut sockets = SocketSet::create(&args.command.socket).await?;
		sockets.serve();
		Some(sockets)
	};

	Ok(Arc::new(InnerState {
		emit_file: RotatingTempFile::default(),
		socket_set,
		exit_code: Mutex::new(ExitCode::SUCCESS),
		watchexec: OnceLock::new(),
	}))
}

#[derive(Debug)]
pub struct InnerState {
	pub emit_file: RotatingTempFile,
	pub socket_set: Option<SocketSet>,
	pub exit_code: Mutex<ExitCode>,
	/// Reference to the Watchexec instance, set after creation.
	/// Used to send synthetic events (e.g., to trigger immediate quit on error).
	pub watchexec: OnceLock<Arc<Watchexec>>,
}

#[derive(Debug, Default)]
pub struct RotatingTempFile(Mutex<Option<NamedTempFile>>);

impl RotatingTempFile {
	pub fn rotate(&self) -> Result<()> {
		// implicitly drops the old file
		*self.0.lock().unwrap() = Some(
			if let Some(dir) = var_os("WATCHEXEC_TMPDIR") {
				NamedTempFile::new_in(dir)
			} else {
				NamedTempFile::new()
			}
			.into_diagnostic()?,
		);
		Ok(())
	}

	pub fn write(&self, data: &[u8]) -> Result<()> {
		if let Some(file) = self.0.lock().unwrap().as_mut() {
			file.write_all(data).into_diagnostic()?;
		}

		Ok(())
	}

	pub fn path(&self) -> PathBuf {
		if let Some(file) = self.0.lock().unwrap().as_ref() {
			file.path().to_owned()
		} else {
			PathBuf::new()
		}
	}
}
