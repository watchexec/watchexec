use std::{
	io::Write,
	path::PathBuf,
	sync::{Arc, Mutex},
};

use miette::{IntoDiagnostic, Result};
use tempfile::NamedTempFile;

#[derive(Clone, Debug)]
pub struct State {
	pub emit_file: RotatingTempFile,
}

impl State {
	pub fn new() -> Result<Self> {
		let emit_file = RotatingTempFile::new()?;
		Ok(Self { emit_file })
	}
}

#[derive(Clone, Debug)]
pub struct RotatingTempFile(Arc<Mutex<NamedTempFile>>);

impl RotatingTempFile {
	pub fn new() -> Result<Self> {
		let file = Arc::new(Mutex::new(NamedTempFile::new().into_diagnostic()?));
		Ok(Self(file))
	}

	pub fn rotate(&self) -> Result<()> {
		let mut file = self.0.lock().unwrap();
		*file = NamedTempFile::new().into_diagnostic()?;
		// implicitly drops the old file
		Ok(())
	}

	pub fn write(&self, data: &[u8]) -> Result<()> {
		let mut file = self.0.lock().unwrap();
		file.write_all(data).into_diagnostic()?;
		Ok(())
	}

	pub fn path(&self) -> PathBuf {
		self.0.lock().unwrap().path().to_owned()
	}
}
