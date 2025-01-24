use std::{
	env::var_os,
	io::Write,
	path::PathBuf,
	sync::{Arc, Mutex},
};

use miette::{IntoDiagnostic, Result};
use tempfile::NamedTempFile;

use crate::socket::SocketSet;

pub type State = Arc<InnerState>;

#[derive(Debug, Default)]
pub struct InnerState {
	pub emit_file: RotatingTempFile,
	pub socket_set: Option<SocketSet>,
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
