//! Command construction and configuration.

#[doc(inline)]
pub use self::{
	program::Program,
	sequence::{Sequence, SequenceTree},
	shell::Shell,
};

mod conversions;
mod program;
mod sequence;
mod shell;

/// A command to execute.
///
/// For simple uses, the `From` and `FromIterator` implementations may be useful:
///
/// ```
/// # use watchexec_supervisor::command::{Command, Program};
/// Command::from(Program::Exec {
///     prog: "ping".into(),
///     args: vec!["-c".into(), "4".into()],
///     grouped: false,
/// });
/// ```
///
/// ```
/// # use watchexec_supervisor::command::{Command, Program, Shell};
/// Command::from_iter(vec![
///     Program::Exec {
///         prog: "nslookup".into(),
///         args: vec!["google.com".into()],
///         grouped: true,
///     },
///     Program::Shell {
///         shell: Shell::new("bash"),
///         command: "curl -L google.com >/dev/null".into(),
///         args: Vec::new(),
///     },
/// ]);
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Command {
	/// Programs to execute as part of this command.
	///
	/// The [`Sequence`] type defines a sequential control flow for the programs, and can represent
	/// such flows as `a && b`, `a || b`, `a; b`, and more. However, pipelines are not supported;
	/// use a shell program for that.
	pub sequence: Sequence,
}
