//! The watchexec v1 filter implementation, using globset.

use std::path::PathBuf;

use crate::error::RuntimeError;
use crate::event::Event;
use crate::filter::Filterer;

#[derive(Debug)]
pub struct GlobsetFilterer {
	_root: PathBuf,
}

impl Filterer for GlobsetFilterer {
	fn check_event(&self, _event: &Event) -> Result<bool, RuntimeError> {
		todo!()
	}
}
