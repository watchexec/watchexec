use std::fmt;

use derive_builder::Builder;

use crate::{error::RuntimeError, handler::Handler};

/// Runtime configuration for [`Watchexec`][crate::Watchexec].
///
/// This is used both when constructing the instance (as initial configuration) and to reconfigure
/// it at runtime via [`Watchexec::reconfig()`][crate::Watchexec::reconfig()].
///
/// Use [`RuntimeConfigBuilder`] to build a new one, or modify an existing one. This struct is
/// marked non-exhaustive such that new options may be added without breaking change.
#[derive(Builder, Clone, Debug)]
#[non_exhaustive]
pub struct RuntimeConfig {
	/// Working data for the filesystem event source.
	///
	/// This notably includes the path set to be watched.
	#[builder(default)]
	pub fs: crate::fs::WorkingData,

	/// Working data for the action processing.
	///
	/// This is the task responsible for scheduling the actions in response to events, applying the
	/// filtering, etc.
	#[builder(default)]
	pub action: crate::action::WorkingData,
}

/// Initialisation configuration for [`Watchexec`][crate::Watchexec].
///
/// This is used only for constructing the instance.
///
/// Use [`InitConfigBuilder`] to build a new one, or modify an existing one. This struct is marked
/// non-exhaustive such that new options may be added without breaking change. Note that this
/// builder uses a different style (consuming `self`) for technical reasons (cannot be `Clone`d).
#[derive(Builder)]
#[builder(pattern = "owned")]
#[non_exhaustive]
pub struct InitConfig {
	/// Runtime error handler.
	///
	/// This is run on every runtime error that occurs within watchexec. By default the placeholder
	/// `()` handler is used, which discards all errors.
	///
	/// If the handler errors, [_that_ error][crate::error::RuntimeError::Handler] is immediately
	/// given to the handler. If that second handler call errors as well, its error is ignored.
	///
	/// Only used at construction time, cannot be changed via reconfiguration.
	#[builder(default = "Box::new(()) as _")]
	pub error_handler: Box<dyn Handler<RuntimeError> + Send>,

	/// Internal: the buffer size of the channel which carries runtime errors.
	///
	/// The default (64) is usually fine. If you expect a much larger throughput of runtime errors,
	/// or if your `error_handler` is slow, adjusting this value may help.
	#[builder(default = "64")]
	pub error_channel_size: usize,

	/// Internal: the buffer size of the channel which carries events.
	///
	/// The default (1024) is usually fine. If you expect a much larger throughput of events,
	/// adjusting this value may help.
	#[builder(default = "1024")]
	pub event_channel_size: usize,
}

impl fmt::Debug for InitConfig {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("InitConfig")
			.field("error_channel_size", &self.error_channel_size)
			.field("event_channel_size", &self.event_channel_size)
			.finish_non_exhaustive()
	}
}
