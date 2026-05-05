use clap::{Parser, ValueEnum};
use miette::Result;

use super::OPTSET_OUTPUT;

#[derive(Debug, Clone, Parser)]
pub struct OutputArgs {
	/// Clear screen before running command
	///
	/// If this doesn't completely clear the screen, try '--clear=reset'.
	#[arg(
		short = 'c',
		long = "clear",
		help_heading = OPTSET_OUTPUT,
		num_args = 0..=1,
		default_missing_value = "clear",
		value_name = "MODE",
		display_order = 30,
	)]
	pub screen_clear: Option<ClearMode>,

	/// Alert when commands start and end
	///
	/// With this, Watchexec will emit a desktop notification when a command starts and ends, on
	/// supported platforms. On unsupported platforms, it may silently do nothing, or log a warning.
	///
	/// The mode can be specified to only notify when the command `start`s, `end`s, or for `both`
	/// (which is the default).
	#[arg(
		short = 'N',
		long,
		help_heading = OPTSET_OUTPUT,
		num_args = 0..=1,
		default_missing_value = "both",
		value_name = "WHEN",
		display_order = 140,
	)]
	pub notify: Option<NotifyMode>,

	/// When to use terminal colours
	///
	/// Setting the environment variable `NO_COLOR` to any value is equivalent to `--color=never`.
	#[arg(
		long,
		help_heading = OPTSET_OUTPUT,
		default_value = "auto",
		value_name = "MODE",
		alias = "colour",
		display_order = 31,
	)]
	pub color: ColourMode,

	/// Print how long the command took to run
	///
	/// This may not be exactly accurate, as it includes some overhead from Watchexec itself. Use
	/// the `time` utility, high-precision timers, or benchmarking tools for more accurate results.
	#[arg(
		long,
		help_heading = OPTSET_OUTPUT,
		display_order = 200,
	)]
	pub timings: bool,

	/// Don't print starting and stopping messages
	///
	/// By default Watchexec will print a message when the command starts and stops. This option
	/// disables this behaviour, so only the command's output, warnings, and errors will be printed.
	#[arg(
		short,
		long,
		help_heading = OPTSET_OUTPUT,
		display_order = 170,
	)]
	pub quiet: bool,

	/// Ring the terminal bell on command completion
	#[arg(
		long,
		help_heading = OPTSET_OUTPUT,
		display_order = 20,
	)]
	pub bell: bool,
}

impl OutputArgs {
	pub(crate) fn normalise(&mut self) -> Result<()> {
		// https://no-color.org/
		if self.color == ColourMode::Auto && std::env::var("NO_COLOR").is_ok() {
			self.color = ColourMode::Never;
		}

		Ok(())
	}
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum ClearMode {
	#[default]
	Clear,
	Reset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ColourMode {
	Auto,
	Always,
	Never,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum NotifyMode {
	/// Notify on both start and end
	#[default]
	Both,
	/// Notify only when the command starts
	Start,
	/// Notify only when the command ends
	End,
}

impl NotifyMode {
	/// Whether to notify on command start
	pub fn on_start(self) -> bool {
		matches!(self, Self::Both | Self::Start)
	}

	/// Whether to notify on command end
	pub fn on_end(self) -> bool {
		matches!(self, Self::Both | Self::End)
	}
}
