use std::time::Duration;

use watchexec_signals::Signal;

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

	/// Sleep for some duration.
	Sleep(Duration),

	/// Send this signal to the command.
	///
	/// This does not wait for the command to complete.
	Signal(Signal),

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

	/// Race both outcomes: run both at once, and when one finishes, cancel the other.
	Race(Box<Outcome>, Box<Outcome>),
}

impl Default for Outcome {
	fn default() -> Self {
		Self::DoNothing
	}
}

impl Outcome {
	/// Convenience function to create an outcome conditional on the state of the subprocess.
	#[must_use]
	pub fn if_running(then: Self, otherwise: Self) -> Self {
		Self::IfRunning(Box::new(then), Box::new(otherwise))
	}

	/// Convenience function to create a sequence of outcomes.
	#[must_use]
	pub fn both(one: Self, two: Self) -> Self {
		Self::Both(Box::new(one), Box::new(two))
	}

	/// Convenience function to create a race of outcomes.
	#[must_use]
	pub fn race(one: Self, two: Self) -> Self {
		Self::Race(Box::new(one), Box::new(two))
	}

	/// Pattern that waits for the subprocess to complete before executing the outcome.
	#[must_use]
	pub fn wait(and_then: Self) -> Self {
		Self::both(Self::Wait, and_then)
	}

	/// Pattern that waits for the subprocess to complete with a timeout.
	#[must_use]
	pub fn wait_timeout(timeout: Duration, and_then: Self) -> Self {
		Self::both(Self::race(Self::Sleep(duration), Self::Wait), and_then)
	}

	/// Resolves the outcome given the current state of the subprocess.
	#[must_use]
	pub fn resolve(self, is_running: bool) -> Self {
		match (is_running, self) {
			(true, Self::IfRunning(then, _)) => then.resolve(true),
			(false, Self::IfRunning(_, otherwise)) => otherwise.resolve(false),
			(ir, Self::Both(one, two)) => Self::both(one.resolve(ir), two.resolve(ir)),
			(_, other) => other,
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn simple_if_running() {
		assert_eq!(
			Outcome::if_running(Outcome::Stop, Outcome::Start).resolve(true),
			Outcome::Stop
		);
		assert_eq!(
			Outcome::if_running(Outcome::Stop, Outcome::Start).resolve(false),
			Outcome::Start
		);
	}

	#[test]
	fn simple_passthrough() {
		assert_eq!(Outcome::Wait.resolve(true), Outcome::Wait);
		assert_eq!(Outcome::Clear.resolve(false), Outcome::Clear);
	}

	#[test]
	fn nested_if_runnings() {
		assert_eq!(
			Outcome::both(
				Outcome::if_running(Outcome::Stop, Outcome::Start),
				Outcome::if_running(Outcome::Wait, Outcome::Exit)
			)
			.resolve(true),
			Outcome::Both(Box::new(Outcome::Stop), Box::new(Outcome::Wait))
		);
	}
}
