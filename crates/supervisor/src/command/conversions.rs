use super::{Command, Program, Sequence};

impl From<Program> for Command {
	fn from(program: Program) -> Self {
		Self {
			sequence: Sequence::from(program),
		}
	}
}

impl FromIterator<Program> for Command {
	fn from_iter<I>(programs: I) -> Self
	where
		I: IntoIterator<Item = Program>,
	{
		Self {
			sequence: Sequence::from_iter(programs),
		}
	}
}

impl From<Program> for Sequence {
	fn from(program: Program) -> Self {
		Self::Run(program)
	}
}

impl FromIterator<Program> for Sequence {
	fn from_iter<I>(programs: I) -> Self
	where
		I: IntoIterator<Item = Program>,
	{
		Self::List(programs.into_iter().map(Self::Run).collect())
	}
}
