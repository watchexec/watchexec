use clap::{Parser, ValueEnum};

use super::OPTSET_DEBUGGING;

#[derive(Debug, Clone, Parser)]
pub struct DebuggingArgs {
	/// Testing only: exit Watchexec after the first run
	#[arg(short = '1', hide = true)]
	pub once: bool,

	/// Print events that trigger actions
	///
	/// This prints the events that triggered the action when handling it (after debouncing), in a
	/// human readable form. This is useful for debugging filters.
	///
	/// Use '-vvv' instead when you need more diagnostic information.
	#[arg(
		long,
		help_heading = OPTSET_DEBUGGING,
	)]
	pub print_events: bool,

	/// Show the manual page
	///
	/// This shows the manual page for Watchexec, if the output is a terminal and the 'man' program
	/// is available. If not, the manual page is printed to stdout in ROFF format (suitable for
	/// writing to a watchexec.1 file).
	#[arg(
		long,
		help_heading = OPTSET_DEBUGGING,
		conflicts_with_all = ["command", "completions"],
	)]
	pub manual: bool,

	/// Generate a shell completions script
	///
	/// Provides a completions script or configuration for the given shell. If Watchexec is not
	/// distributed with pre-generated completions, you can use this to generate them yourself.
	///
	/// Supported shells: bash, elvish, fish, nu, powershell, zsh.
	#[arg(
		long,
		help_heading = OPTSET_DEBUGGING,
		conflicts_with_all = ["command", "manual"],
	)]
	pub completions: Option<ShellCompletion>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ShellCompletion {
	Bash,
	Elvish,
	Fish,
	Nu,
	Powershell,
	Zsh,
}
