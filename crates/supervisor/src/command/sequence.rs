use std::{collections::VecDeque};

use super::Program;

/// A sequence of programs, with some control flow.
///
/// This is effectively a hybrid tree of programs and subsequences, and can be thought of as the AST
/// that would result from parsing a command line.
///
/// # Examples
///
/// For simple uses, the `From` and `FromIterator` implementations may be useful:
///
/// ```
/// # use watchexec_supervisor::command::{Sequence, Program};
/// Sequence::from(Program::Exec {
///     prog: "ping".into(),
///     args: vec!["-c".into(), "4".into()],
/// });
/// ```
///
/// ```
/// # use watchexec_supervisor::command::{Sequence, Program, Shell};
/// Sequence::from_iter(vec![
///     Program::Exec {
///         prog: "nslookup".into(),
///         args: vec!["google.com".into()],
///     },
///     Program::Shell {
///         shell: Shell::new("bash"),
///         command: "curl -L google.com >/dev/null".into(),
///         args: Vec::new(),
///     },
/// ]);
/// ```
///
/// For more complex uses, the `and`, `or`, and `andor` methods may be useful:
///
/// ```
/// # use watchexec_supervisor::command::{Sequence, Program, Shell};
/// Sequence::Run(Program::Exec {
///     prog: "nslookup".into(),
///     args: vec!["google.com".into()],
/// }).and(Program::Shell {
///     shell: Shell::new("bash"),
///     command: "curl -L google.com >/dev/null".into(),
///     args: Vec::new(),
/// }.into());
/// // = `nslookup google.com && curl -L google.com >/dev/null`
/// ```
///
/// ```
/// # use watchexec_supervisor::command::{Sequence, Program, Shell};
/// Sequence::Run(Program::Shell {
///     shell: Shell::new("bash"),
///     command: "make test".into(),
///     args: Vec::new(),
/// }).or(Program::Shell {
///     shell: Shell::new("bash"),
///     command: "make fix".into(),
///     args: Vec::new(),
/// }.into());
/// // = `make test || make fix`
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Sequence {
	/// A single program to execute.
	Run(Program),

	/// A list of subsequences to execute sequentially.
	///
	/// If a sequence fails, the next sequence is started. If all sequences fail, this sequence fails.
	///
	/// It is equivalent to `a; b; c; ...`.
	List(VecDeque<Sequence>),

	/// A conditional control.
	///
	/// If the `given` sequence succeeds, the `then` sequence is executed, and its status bubbles up.
	/// If it fails, the `otherwise` sequence is executed, and its status bubbles up. If there is no
	/// sequence for the outcome of `given`, the status of `given` bubbles up.
	///
	/// This can represent `a && b` (with `otherwise` empty), `a || b` (with `then` empty), and
	/// `a && b || c` (both `then` and `otherwise` provided).
	///
	/// If neither `then` nor `otherwise` are provided, this is equivalent to the `given` sequence.
	Condition {
		given: Box<Sequence>,
		then: Option<Box<Sequence>>,
		otherwise: Option<Box<Sequence>>,
	}
}

impl Default for Sequence {
	fn default() -> Self {
		Self::List(VecDeque::new())
	}
}

impl Sequence {
	fn condition(given: Self, then: Option<Self>, otherwise: Option<Self>) -> Self {
		Self::Condition {
			given: Box::new(given),
			then: then.map(Box::new),
			otherwise: otherwise.map(Box::new),
		}
	}

	pub fn and(self, then: Self) -> Self {
		Self::condition(self, Some(then), None)
	}

	pub fn or(self, otherwise: Self) -> Self {
		Self::condition(self, None, Some(otherwise))
	}

	pub fn andor(self, then: Self, otherwise: Self) -> Self {
		Self::condition(self, Some(then), Some(otherwise))
	}
}
