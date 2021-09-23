use crate::{error::RuntimeError, event::Event};

pub mod globset;
pub mod tagged;

pub trait Filterer: Send + Sync {
	fn check_event(&self, event: &Event) -> Result<bool, RuntimeError>;
}

impl Filterer for () {
	fn check_event(&self, _event: &Event) -> Result<bool, RuntimeError> {
		Ok(true)
	}
}
