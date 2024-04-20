use std::{
	io::Write,
	path::PathBuf,
	sync::{Arc, Mutex},
};

use miette::{IntoDiagnostic, Result};
use tempfile::NamedTempFile;

#[derive(Clone, Debug, Default)]
pub struct State {
	pub emit_file: RotatingTempFile,
}

#[derive(Clone, Debug, Default)]
pub struct RotatingTempFile(Arc<Mutex<Option<NamedTempFile>>>);

impl RotatingTempFile {
	pub fn rotate(&self) -> Result<()> {
		// implicitly drops the old file
		*self.0.lock().unwrap() = Some(NamedTempFile::new().into_diagnostic()?);
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
