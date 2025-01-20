use clap::Parser;

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
}
