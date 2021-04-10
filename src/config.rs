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

use std::path::PathBuf;

use crate::process::Shell;

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
    /// If true, kill the command if it's still running when a change comes in.
    #[builder(default)]
    pub restart: bool,
    /// Interval to debounce the changes. (milliseconds)
    #[builder(default = "500")]
    pub debounce: u64,
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
    /// Interval for polling. (milliseconds)
    #[builder(default = "1000")]
    pub poll_interval: u32,
    /// Ignore events emitted while the command is running.
    #[builder(default)]
    pub watch_when_idle: bool,
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
}
