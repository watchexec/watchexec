//! Configuration and builders for [`crate::Watchexec`].

use std::{path::Path, sync::Arc, time::Duration};

use tracing::debug;

use crate::{
	action::{Action, PostSpawn, PreSpawn},
	filter::Filterer,
	fs::Watcher,
	handler::HandlerLock,
	ErrorHook,
};

/// Configuration for [`Watchexec`][crate::Watchexec].
///
/// This is used both when constructing the instance (as initial configuration) and to reconfigure
/// it at runtime via [`Watchexec::reconfigure()`][crate::Watchexec::reconfigure()].
///
/// Use [`Config::default()`] to build a new one, or modify an existing one. This struct is
/// marked non-exhaustive such that new options may be added without breaking change. You can make
/// changes through the fields directly, or use the convenience (chainable!) methods instead.
///
/// Another advantage of using the convenience methods is that each one contains a call to the
/// [`debug!`] macro, providing insight into what config your application sets for "free".
///
/// You should see the detailed documentation on [`fs::WorkingData`][crate::fs::WorkingData] and
/// [`action::WorkingData`][crate::action::WorkingData] for important information and particulars
/// about each field, especially the handlers.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Config {
	/// Runtime error handler.
	///
	/// This is run on every runtime error that occurs within Watchexec. The default handler
	/// is a no-op.
	///
	/// # Examples
	///
	/// Set the error handler:
	///
	/// ```
	/// # use watchexec::{config::Config, ErrorHook};
	/// let mut config = Config::default();
	/// config.on_error(|err: ErrorHook| {
	///     tracing::error!("{}", err.error);
	/// });
	/// ```
	///
	/// Output a critical error (which will terminate Watchexec):
	///
	/// ```
	/// # use watchexec::{config::Config, ErrorHook, error::{CriticalError, RuntimeError}};
	/// let mut config = Config::default();
	/// config.on_error(|err: ErrorHook| {
	///     tracing::error!("{}", err.error);
	///
	///     if matches!(err.error, RuntimeError::FsWatcher { .. }) {
	///         err.critical(CriticalError::External("fs watcher failed".into()));
	///     }
	/// });
	/// ```
	///
	/// Elevate a runtime error to critical (will preserve the error information):
	///
	/// ```
	/// # use watchexec::{config::Config, ErrorHook, error::RuntimeError};
	/// let mut config = Config::default();
	/// config.on_error(|err: ErrorHook| {
	///     tracing::error!("{}", err.error);
	///
	///     if matches!(err.error, RuntimeError::FsWatcher { .. }) {
	///            err.elevate();
	///     }
	/// });
	/// ```
	pub error_handler: HandlerLock<ErrorHook>,

	/// Working data for the filesystem event source.
	///
	/// This notably includes the path set to be watched.
	pub fs: crate::fs::WorkingData,

	/// Working data for keyboard event sources.
	pub keyboard: crate::keyboard::WorkingData,

	/// Working data for the action processing.
	///
	/// This is the task responsible for scheduling the actions in response to events, applying the
	/// filtering, etc.
	pub action: crate::action::WorkingData,
}

impl Config {
	/// Set the pathset to be watched.
	pub fn pathset<I, P>(&mut self, pathset: I) -> &mut Self
	where
		I: IntoIterator<Item = P>,
		P: AsRef<Path>,
	{
		self.fs.pathset = pathset.into_iter().map(|p| p.as_ref().into()).collect();
		debug!(pathset=?self.fs.pathset, "Config: pathset");
		self
	}

	/// Set the file watcher type to use.
	pub fn file_watcher(&mut self, watcher: Watcher) -> &mut Self {
		debug!(?watcher, "Config: watcher");
		self.fs.watcher = watcher;
		self
	}

	/// Enable monitoring of 'end of file' from stdin
	pub fn keyboard_emit_eof(&mut self, enable: bool) -> &mut Self {
		debug!(?enable, "Config: keyboard");
		self.keyboard.eof = enable;
		self
	}

	/// Set the action throttle.
	pub fn action_throttle(&mut self, throttle: impl Into<Duration>) -> &mut Self {
		self.action.throttle = throttle.into();
		debug!(throttle=?self.action.throttle, "Config: throttle");
		self
	}

	/// Set the filterer implementation to use.
	pub fn filterer(&mut self, filterer: Arc<dyn Filterer>) -> &mut Self {
		debug!(?filterer, "Config: filterer");
		self.action.filterer = filterer;
		self
	}

	/// Set the runtime error handler.
	pub fn on_error(&mut self, handler: impl FnMut(ErrorHook) + Send + 'static) -> &mut Self {
		debug!("Config: on_error");
		self.error_handler.replace(Box::new(handler));
		self
	}

	/// Set the action handler.
	pub fn on_action(&mut self, handler: impl FnMut(Action) + Send + 'static) -> &mut Self {
		debug!("Config: on_action");
		self.action.action_handler.replace(Box::new(handler));
		self
	}

	/// Set the pre-spawn handler.
	pub fn on_pre_spawn(&mut self, handler: impl FnMut(PreSpawn) + Send + 'static) -> &mut Self {
		debug!("Config: on_pre_spawn");
		self.action.pre_spawn_handler.replace(Box::new(handler));
		self
	}

	/// Set the post-spawn handler.
	pub fn on_post_spawn(&mut self, handler: impl FnMut(PostSpawn) + Send + 'static) -> &mut Self {
		debug!("Config: on_post_spawn");
		self.action.post_spawn_handler.replace(Box::new(handler));
		self
	}
}

/// Internal configuration for [`Watchexec`][crate::Watchexec].
///
/// These are internal details that you may want to tune in some situations.
///
/// Use [`InitConfig::default()`] to build a new one, and the inherent methods to change values.
/// This struct is marked non-exhaustive such that new options may be added without breaking change.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct InternalConfig {
	/// The buffer size of the channel which carries runtime errors.
	///
	/// The default (64) is usually fine. If you expect a much larger throughput of runtime errors,
	/// or if your `error_handler` is slow, adjusting this value may help.
	pub error_channel_size: usize,

	/// The buffer size of the channel which carries events.
	///
	/// The default (4096) is usually fine. If you expect a much larger throughput of events,
	/// adjusting this value may help.
	pub event_channel_size: usize,
}

impl Default for InternalConfig {
	fn default() -> Self {
		Self {
			error_channel_size: 64,
			event_channel_size: 4096,
		}
	}
}

impl InternalConfig {
	/// Set the buffer size of the channel which carries runtime errors.
	///
	/// See the [documentation on the field](InternalConfig#structfield.error_channel_size) for more details.
	pub fn error_channel_size(&mut self, size: usize) -> &mut Self {
		debug!(?size, "InternalConfig: error_channel_size");
		self.error_channel_size = size;
		self
	}

	/// Set the buffer size of the channel which carries events.
	///
	/// See the [documentation on the field](InternalConfig#structfield.event_channel_size) for more details.
	pub fn event_channel_size(&mut self, size: usize) -> &mut Self {
		debug!(?size, "InternalConfig: event_channel_size");
		self.event_channel_size = size;
		self
	}
}
