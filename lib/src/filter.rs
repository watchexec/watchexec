use std::sync::Arc;

use crate::{error::RuntimeError, event::Event};

pub mod globset;
pub mod tagged;

pub trait Filterer: std::fmt::Debug + Send + Sync {
	fn check_event(&self, event: &Event) -> Result<bool, RuntimeError>;
}

impl Filterer for () {
	fn check_event(&self, _event: &Event) -> Result<bool, RuntimeError> {
		Ok(true)
	}
}

impl<T: Filterer> Filterer for Arc<T> {
	fn check_event(&self, event: &Event) -> Result<bool, RuntimeError> {
		Arc::as_ref(self).check_event(event)
	}
}
