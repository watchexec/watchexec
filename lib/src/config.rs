use derive_builder::Builder;

/// Configuration for [`Watchexec`][crate::Watchexec].
///
/// This is used both for constructing the instance and to reconfigure it at runtime, though note
/// that some fields are only applied at construction time.
///
/// Use [`ConfigBuilder`] to build a new one, or modify an existing one. This struct is marked
/// non-exhaustive such that new options may be added without breaking change.
#[derive(Builder, Clone, Debug)]
#[non_exhaustive]
pub struct Config {
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

	/// Internal: the buffer size of the channel which carries runtime errors.
	///
	/// The default (64) is usually fine. If you expect a much larger throughput of runtime errors,
	/// adjusting this value may help. (Fixing whatever is causing the errors may also help.)
	///
	/// Only used at construction time, cannot be changed via reconfiguration.
	#[builder(default = "64")]
	pub error_channel_size: usize,

	/// Internal: the buffer size of the channel which carries events.
	///
	/// The default (1024) is usually fine. If you expect a much larger throughput of events,
	/// adjusting this value may help.
	///
	/// Only used at construction time, cannot be changed via reconfiguration.
	#[builder(default = "1024")]
	pub event_channel_size: usize,
}
