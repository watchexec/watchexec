use std::path::PathBuf;

use crate::error::RuntimeError;
use crate::event::Event;
use crate::filter::Filterer;

#[doc(inline)]
pub use types::*;

mod parse;
mod types;

pub struct TaggedFilterer {
	_root: PathBuf,
}

impl Filterer for TaggedFilterer {
	fn check_event(&self, _event: &Event) -> Result<bool, RuntimeError> {
		todo!()
	}
}
