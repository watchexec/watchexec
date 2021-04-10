//! Configuration for watchexec.
//!
//! The [`Config`] struct is not constructable, use [`ConfigBuilder`].
//!
//! # Examples
//!
//! ```
//! # use watchexec::config::ConfigBuilder;
//! ConfigBuilder::default()
//!     .cmd(vec!["echo hello world".into()])
//!     .paths(vec![".".into()])
//!     .build()
//!     .expect("mission failed");
//! ```

use std::{time::Duration, path::PathBuf};

use crate::process::Shell;
use crate::run::OnBusyUpdate;

/// Arguments to the watcher
#[derive(Builder, Clone, Debug)]
#[builder(setter(into, strip_option))]
#[builder(build_fn(validate = "Self::validate"))]
#[non_exhaustive]
pub struct Config {
    /// Command to execute in popen3 format (first program, rest arguments).
    pub cmd: Vec<String>,
    /// List of paths to watch for changes.
    pub paths: Vec<PathBuf>,
    /// Positive filters (trigger only on matching changes). Glob format.
    #[builder(default)]
    pub filters: Vec<String>,
    /// Negative filters (do not trigger on matching changes). Glob format.
    #[builder(default)]
    pub ignores: Vec<String>,
    /// Clear the screen before each run.
    #[builder(default)]
    pub clear_screen: bool,
    /// If Some, send that signal (e.g. SIGHUP) to the command on change.
    #[builder(default)]
    pub signal: Option<String>,
    /// Specify what to do when receiving updates while the command is running.
    #[builder(default)]
    pub on_busy_update: OnBusyUpdate,
    /// Interval to debounce the changes.
    #[builder(default = "Duration::from_millis(500)")]
    pub debounce: Duration,
    /// Run the commands right after starting.
    #[builder(default = "true")]
    pub run_initially: bool,
    /// Specify the shell to use.
    #[builder(default)]
    pub shell: Shell,
    /// Ignore metadata changes.
    #[builder(default)]
    pub no_meta: bool,
    /// Do not set WATCHEXEC_*_PATH environment variables for the process.
    #[builder(default)]
    pub no_environment: bool,
    /// Skip auto-loading .gitignore files
    #[builder(default)]
    pub no_vcs_ignore: bool,
    /// Skip auto-loading .ignore files
    #[builder(default)]
    pub no_ignore: bool,
    /// For testing only, always set to false.
    #[builder(setter(skip))]
    #[builder(default)]
    #[doc(hidden)]
    pub once: bool,
    /// Force using the polling backend.
    #[builder(default)]
    pub poll: bool,
    /// Interval for polling.
    #[builder(default = "Duration::from_secs(1)")]
    pub poll_interval: Duration,
}

impl ConfigBuilder {
    fn validate(&self) -> Result<(), String> {
        if self.cmd.as_ref().map_or(true, Vec::is_empty) {
            return Err("cmd must not be empty".into());
        }

        if self.paths.as_ref().map_or(true, Vec::is_empty) {
            return Err("paths must not be empty".into());
        }

        Ok(())
    }

    #[deprecated(since = "1.15.0", note = "does nothing. set the log level instead")]
    pub fn debug(&mut self, _: impl Into<bool>) -> &mut Self {
        self
    }

    /// Do not wrap the commands in a shell.
    #[deprecated(since = "1.15.0", note = "use shell(Shell::None) instead")]
    pub fn no_shell(&mut self, s: impl Into<bool>) -> &mut Self {
        if s.into() {
            self.shell(Shell::default())
        } else {
            self.shell(Shell::None)
        }
    }

    #[deprecated(since = "1.15.0", note = "use on_busy_update(Restart) instead")]
    pub fn restart(&mut self, b: impl Into<bool>) -> &mut Self {
        if b.into() {
            self.on_busy_update(OnBusyUpdate::Restart)
        } else {
            self
        }
    }

    #[deprecated(since = "1.15.0", note = "use on_busy_update(DoNothing) instead")]
    pub fn watch_when_idle(&mut self, b: impl Into<bool>) -> &mut Self {
        if b.into() {
            self.on_busy_update(OnBusyUpdate::DoNothing)
        } else {
            self
        }
    }
}
