use crate::signal::process::SubSignal;

/// The outcome to execute when an action is triggered.
///
/// Logic against the state of the command should be expressed using these variants, rather than
/// inside the action handler, as it ensures the state of the command is always the latest available
/// when the outcome is executed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Outcome {
	/// Stop processing this action silently.
	DoNothing,

	/// If the command is running, stop it.
	///
	/// This should be used with an `IfRunning`, and will warn if the command is not running.
	Stop,

	/// If the command isn't running, start it.
	///
	/// This should be used with an `IfRunning`, and will warn if the command is running.
	Start,

	/// Wait for command completion.
	///
	/// Does nothing if the command isn't running.
	Wait,

	/// Send this signal to the command.
	///
	/// This does not wait for the command to complete.
	Signal(SubSignal),

	/// Clear the (terminal) screen.
	Clear,

	/// Reset the (terminal) screen.
	///
	/// This invokes (in order): [`WindowsCooked`][clearscreen::ClearScreen::WindowsCooked],
	/// [`WindowsVt`][clearscreen::ClearScreen::WindowsVt],
	/// [`VtLeaveAlt`][clearscreen::ClearScreen::VtLeaveAlt],
	/// [`VtWellDone`][clearscreen::ClearScreen::VtWellDone],
	/// and [the default clear][clearscreen::ClearScreen::default()].
	Reset,

	/// Exit watchexec.
	Exit,

	/// When command is running, do the first, otherwise the second.
	IfRunning(Box<Outcome>, Box<Outcome>),

	/// Do both outcomes in order.
	Both(Box<Outcome>, Box<Outcome>),
}

impl Default for Outcome {
	fn default() -> Self {
		Self::DoNothing
	}
}

impl Outcome {
	/// Convenience function to create an outcome conditional on the state of the subprocess.
	pub fn if_running(then: Outcome, otherwise: Outcome) -> Self {
		Self::IfRunning(Box::new(then), Box::new(otherwise))
	}

	/// Convenience function to create a sequence of outcomes.
	pub fn both(one: Outcome, two: Outcome) -> Self {
		Self::Both(Box::new(one), Box::new(two))
	}

	/// Convenience function to wait for the subprocess to complete before executing the outcome.
	pub fn wait(and_then: Outcome) -> Self {
		Self::Both(Box::new(Outcome::Wait), Box::new(and_then))
	}

	/// Resolves the outcome given the current state of the subprocess.
	pub fn resolve(self, is_running: bool) -> Self {
		match (is_running, self) {
			(true, Self::IfRunning(then, _)) => then.resolve(true),
			(false, Self::IfRunning(_, otherwise)) => otherwise.resolve(false),
			(ir, Self::Both(one, two)) => Self::both(one.resolve(ir), two.resolve(ir)),
			(_, other) => other,
		}
	}
}
