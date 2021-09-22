#[doc(inline)]
pub use types::*;

use crate::{error::RuntimeError, event::Event};

mod parse;
mod types;

pub trait Filterer: Send + Sync {
	fn check_event(&self, event: &Event) -> Result<bool, RuntimeError>;
}

impl Filterer for () {
	fn check_event(&self, _event: &Event) -> Result<bool, RuntimeError> {
		Ok(true)
	}
}
