//! Configuration and builders for [`crate::Watchexec`].

use std::{fmt, path::Path, sync::Arc, time::Duration};

use tracing::debug;

use crate::{
	action::{Action, PostSpawn, PreSpawn},
	command::Command,
	filter::Filterer,
	fs::Watcher,
	handler::{Handler, HandlerLock},
	ErrorHook,
};

/// Runtime configuration for [`Watchexec`][crate::Watchexec].
///
/// This is used both when constructing the instance (as initial configuration) and to reconfigure
/// it at runtime via [`Watchexec::reconfigure()`][crate::Watchexec::reconfigure()].
///
/// Use [`RuntimeConfig::default()`] to build a new one, or modify an existing one. This struct is
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
pub struct RuntimeConfig {
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

impl RuntimeConfig {
	/// Set the pathset to be watched.
	pub fn pathset<I, P>(&mut self, pathset: I) -> &mut Self
	where
		I: IntoIterator<Item = P>,
		P: AsRef<Path>,
	{
		self.fs.pathset = pathset.into_iter().map(|p| p.as_ref().into()).collect();
		debug!(pathset=?self.fs.pathset, "RuntimeConfig: pathset");
		self
	}

	/// Set the file watcher type to use.
	pub fn file_watcher(&mut self, watcher: Watcher) -> &mut Self {
		debug!(?watcher, "RuntimeConfig: watcher");
		self.fs.watcher = watcher;
		self
	}

	/// Enable monitoring of 'end of file' from stdin
	pub fn keyboard_emit_eof(&mut self, enable: bool) -> &mut Self {
		self.keyboard.eof = enable;
		self
	}

	/// Set the action throttle.
	pub fn action_throttle(&mut self, throttle: impl Into<Duration>) -> &mut Self {
		self.action.throttle = throttle.into();
		debug!(throttle=?self.action.throttle, "RuntimeConfig: throttle");
		self
	}

	/// Toggle whether to use process groups or not.
	pub fn command_grouped(&mut self, grouped: bool) -> &mut Self {
		debug!(?grouped, "RuntimeConfig: command_grouped");
		self.action.grouped = grouped;
		self
	}

	/// Set a single command to run on action.
	///
	/// This is a convenience for `.commands(vec![Command...])`.
	pub fn command(&mut self, command: Command) -> &mut Self {
		debug!(?command, "RuntimeConfig: command");
		self.action.commands = vec![command];
		self
	}

	/// Set the commands to run on action.
	pub fn commands(&mut self, commands: impl Into<Vec<Command>>) -> &mut Self {
		self.action.commands = commands.into();
		debug!(commands=?self.action.commands, "RuntimeConfig: commands");
		self
	}

	/// Set the filterer implementation to use.
	pub fn filterer(&mut self, filterer: Arc<dyn Filterer>) -> &mut Self {
		debug!(?filterer, "RuntimeConfig: filterer");
		self.action.filterer = filterer;
		self
	}

	/// Set the action handler.
	pub fn on_action(&mut self, handler: impl Handler<Action> + Send + 'static) -> &mut Self {
		debug!("RuntimeConfig: on_action");
		self.action.action_handler = HandlerLock::new(Box::new(handler));
		self
	}

	/// Set the pre-spawn handler.
	pub fn on_pre_spawn(&mut self, handler: impl Handler<PreSpawn> + Send + 'static) -> &mut Self {
		debug!("RuntimeConfig: on_pre_spawn");
		self.action.pre_spawn_handler = HandlerLock::new(Box::new(handler));
		self
	}

	/// Set the post-spawn handler.
	pub fn on_post_spawn(
		&mut self,
		handler: impl Handler<PostSpawn> + Send + 'static,
	) -> &mut Self {
		debug!("RuntimeConfig: on_post_spawn");
		self.action.post_spawn_handler = HandlerLock::new(Box::new(handler));
		self
	}
}

/// Initialisation configuration for [`Watchexec`][crate::Watchexec].
///
/// This is used only for constructing the instance.
///
/// Use [`InitConfig::default()`] to build a new one, and the inherent methods to change values.
/// This struct is marked non-exhaustive such that new options may be added without breaking change.
#[non_exhaustive]
pub struct InitConfig {
	/// Runtime error handler.
	///
	/// This is run on every runtime error that occurs within watchexec. By default the placeholder
	/// `()` handler is used, which discards all errors.
	///
	/// If the handler errors, [_that_ error][crate::error::RuntimeError::Handler] is immediately
	/// given to the handler. If this second handler call errors as well, its error is ignored.
	///
	/// Also see the [`ErrorHook`] documentation for returning critical errors from this handler.
	///
	/// # Examples
	///
	/// ```
	/// # use std::convert::Infallible;
	/// # use watchexec::{config::InitConfig, ErrorHook};
	/// let mut init = InitConfig::default();
	/// init.on_error(|err: ErrorHook| async move {
	///     tracing::error!("{}", err.error);
	///     Ok::<(), Infallible>(())
	/// });
	/// ```
	pub error_handler: Box<dyn Handler<ErrorHook> + Send>,

	/// Internal: the buffer size of the channel which carries runtime errors.
	///
	/// The default (64) is usually fine. If you expect a much larger throughput of runtime errors,
	/// or if your `error_handler` is slow, adjusting this value may help.
	pub error_channel_size: usize,

	/// Internal: the buffer size of the channel which carries events.
	///
	/// The default (1024) is usually fine. If you expect a much larger throughput of events,
	/// adjusting this value may help.
	pub event_channel_size: usize,
}

impl Default for InitConfig {
	fn default() -> Self {
		Self {
			error_handler: Box::new(()) as _,
			error_channel_size: 64,
			event_channel_size: 1024,
		}
	}
}

impl InitConfig {
	/// Set the runtime error handler.
	///
	/// See the [documentation on the field](InitConfig#structfield.error_handler) for more details.
	pub fn on_error(&mut self, handler: impl Handler<ErrorHook> + Send + 'static) -> &mut Self {
		debug!("InitConfig: on_error");
		self.error_handler = Box::new(handler) as _;
		self
	}

	/// Set the buffer size of the channel which carries runtime errors.
	///
	/// See the [documentation on the field](InitConfig#structfield.error_channel_size) for more details.
	pub fn error_channel_size(&mut self, size: usize) -> &mut Self {
		debug!(?size, "InitConfig: error_channel_size");
		self.error_channel_size = size;
		self
	}

	/// Set the buffer size of the channel which carries events.
	///
	/// See the [documentation on the field](InitConfig#structfield.event_channel_size) for more details.
	pub fn event_channel_size(&mut self, size: usize) -> &mut Self {
		debug!(?size, "InitConfig: event_channel_size");
		self.event_channel_size = size;
		self
	}
}

impl fmt::Debug for InitConfig {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("InitConfig")
			.field("error_channel_size", &self.error_channel_size)
			.field("event_channel_size", &self.event_channel_size)
			.finish_non_exhaustive()
	}
}
