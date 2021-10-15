use crate::signal::process::SubSignal;

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Outcome {
	/// Stop processing this action silently.
	DoNothing,

	/// If the command is running, stop it.
	Stop,

	/// If the command isn't running, start it.
	Start,

	/// Wait for command completion.
	Wait,

	/// Send this signal to the command.
	Signal(SubSignal),

	/// Clear the (terminal) screen.
	Clear,

	/// Reset the (terminal) screen.
	///
	/// This invokes (in order): [`WindowsCooked`][ClearScreen::WindowsCooked],
	/// [`WindowsVt`][ClearScreen::WindowsVt], [`VtLeaveAlt`][ClearScreen::VtLeaveAlt],
	/// [`VtWellDone`][ClearScreen::VtWellDone], and [the default][ClearScreen::default()].
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
	pub fn if_running(then: Outcome, otherwise: Outcome) -> Self {
		Self::IfRunning(Box::new(then), Box::new(otherwise))
	}

	pub fn both(one: Outcome, two: Outcome) -> Self {
		Self::Both(Box::new(one), Box::new(two))
	}

	pub fn wait(and_then: Outcome) -> Self {
		Self::Both(Box::new(Outcome::Wait), Box::new(and_then))
	}

	pub fn resolve(self, is_running: bool) -> Self {
		match (is_running, self) {
			(true, Self::IfRunning(then, _)) => then.resolve(true),
			(false, Self::IfRunning(_, otherwise)) => otherwise.resolve(false),
			(ir, Self::Both(one, two)) => Self::both(one.resolve(ir), two.resolve(ir)),
			(_, other) => other,
		}
	}
}
