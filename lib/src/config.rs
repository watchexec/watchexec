//! Configuration and builders for [`crate::Watchexec`].

use std::{fmt, path::Path, sync::Arc, time::Duration};

use atomic_take::AtomicTake;
use derive_builder::Builder;

use crate::{action::Action, command::Shell, error::RuntimeError, fs::Watcher, handler::Handler};

/// Runtime configuration for [`Watchexec`][crate::Watchexec].
///
/// This is used both when constructing the instance (as initial configuration) and to reconfigure
/// it at runtime via [`Watchexec::reconfigure()`][crate::Watchexec::reconfigure()].
///
/// Use [`RuntimeConfig::default()`] to build a new one, or modify an existing one. This struct is
/// marked non-exhaustive such that new options may be added without breaking change. You can make
/// changes through the fields directly, or use the convenience (chainable!) methods instead.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct RuntimeConfig {
	/// Working data for the filesystem event source.
	///
	/// This notably includes the path set to be watched.
	pub fs: crate::fs::WorkingData,

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
		self
	}

	/// Set the file watcher type to use.
	pub fn file_watcher(&mut self, watcher: Watcher) -> &mut Self {
		self.fs.watcher = watcher;
		self
	}

	/// Set the action throttle.
	pub fn action_throttle(&mut self, throttle: impl Into<Duration>) -> &mut Self {
		self.action.throttle = throttle.into();
		self
	}

	/// Set the shell to use to invoke commands.
	pub fn command_shell(&mut self, shell: Shell) -> &mut Self {
		self.action.shell = shell;
		self
	}

	/// Toggle whether to use process groups or not.
	pub fn command_grouped(&mut self, grouped: bool) -> &mut Self {
		self.action.grouped = grouped;
		self
	}

	/// Set the command to run on action.
	pub fn command<'cmd>(&mut self, command: impl IntoIterator<Item = &'cmd str>) -> &mut Self {
		self.action.command = command.into_iter().map(|c| c.to_owned()).collect();
		self
	}

	/// Set the action handler.
	///
	/// TODO: notes on how outcome is read immediately after handler returns
	pub fn on_action(&mut self, handler: impl Handler<Action> + Send + 'static) -> &mut Self {
		self.action.action_handler = Arc::new(AtomicTake::new(Box::new(handler) as _));
		self
	}
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
	/// given to the handler. If this second handler call errors as well, its error is ignored.
	///
	/// # Examples
	///
	/// ```
	/// # use std::convert::Infallible;
	/// # use watchexec::config::InitConfigBuilder;
	/// let mut init = InitConfigBuilder::default();
	/// init.on_error(|err| async move {
	///     tracing::error!("{}", err);
	///     Ok::<(), Infallible>(())
	/// });
	/// ```
	#[builder(private, default = "Box::new(()) as _")]
	// TODO: figure out how to remove the builder setter entirely
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

impl InitConfigBuilder {
	/// Set the runtime error handler.
	///
	/// See the [documentation on the field][InitConfig#structfield.error_handler] for more details.
	pub fn on_error(&mut self, handler: impl Handler<RuntimeError> + Send + 'static) -> &mut Self {
		self.error_handler = Some(Box::new(handler) as _);
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
