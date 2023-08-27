use std::time::Duration;

use watchexec_signals::Signal;

use crate::{action::PreSpawn, changeable::ChangeableFn};

/// The outcome to execute when an action is triggered.
///
/// Logic against the state of the command should be expressed using these variants, rather than
/// inside the action handler, as it ensures the state of the command is always the latest available
/// when the outcome is executed.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub enum Outcome {
	/// Does nothing.
	///
	/// This can be used as a default action, or as one branch in a conditional.
	#[default]
	DoNothing,

	/// If the command is running, stop it.
	///
	/// This should be used with an `IfRunning`, and will warn if the command is not running.
	/// TODO: don't warn
	Stop,

	/// If the command isn't running, start it.
	///
	/// This should be used with an `IfRunning`, and will warn if the command is running.
	/// TODO: don't warn
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

	/// Call a custom handler.
	///
	/// TODO: document synchronicity, requirements, and payload?
	///
	/// `PartialEq` ignores the value of this variant.
	Hook(ChangeableFn<()>),

	/// Start the command, calling a custom PreSpawn handler.
	///
	/// `PartialEq` ignores the value of this variant.
	StartHook(ChangeableFn<PreSpawn>),

	/// Destroy the supervisor.
	///
	/// This implies stopping the command if it's still running.
	///
	/// In an action handler, prefer using `remove()` instead of specifying this in `apply()`.
	Destroy,

	/// Exit watchexec.
	Exit,

	/// When command is running, do the first, otherwise the second.
	IfRunning(Box<Outcome>, Box<Outcome>),

	/// Do both outcomes in order.
	Both(Box<Outcome>, Box<Outcome>),

	/// Race both outcomes: run both at once, and when one finishes, cancel the other.
	Race(Box<Outcome>, Box<Outcome>),
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

	/// Pattern that creates a sequence of outcomes from an iterator.
	#[must_use]
	pub fn sequence(mut outcomes: impl Iterator<Item = Self>) -> Self {
		let mut seq = outcomes.next().unwrap_or(Self::DoNothing);
		for outcome in outcomes {
			seq = Self::both(seq, outcome);
		}
		seq
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
		Self::both(Self::race(Self::Sleep(timeout), Self::Wait), and_then)
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

impl PartialEq for Outcome {
	fn eq(&self, other: &Self) -> bool {
		use Outcome::*;
		match (self, other) {
			(DoNothing, DoNothing)
			| (Stop, Stop)
			| (Start, Start)
			| (Wait, Wait)
			| (Clear, Clear)
			| (Reset, Reset)
			| (Exit, Exit) => true,
			(Sleep(a), Sleep(b)) => a == b,
			(Signal(a), Signal(b)) => a == b,
			(IfRunning(aa, ab), IfRunning(ba, bb)) | (Both(aa, ab), Both(ba, bb)) => {
				aa == ba && ab == bb
			}
			(Race(aa, ab), Race(ba, bb)) => (aa == ba && ab == bb) || (aa == bb && ab == ba),
			(Hook(_), Hook(_)) | (StartHook(_), StartHook(_)) => true,
			_ => false,
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
