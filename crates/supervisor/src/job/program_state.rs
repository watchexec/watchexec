use std::time::Instant;

use command_group::stdlib::ErasedChild;
use watchexec_events::ProcessEnd;

use crate::command::{Program, SequenceTree};

#[derive(Debug)]
pub enum ProgramState {
	ToRun(Program),
	IsRunning {
		program: Program,
		child: ErasedChild,
		started: Instant,
	},
	Finished {
		program: Program,
		status: ProcessEnd,
		started: Instant,
		finished: Instant,
	},
}

pub type StateSequence = SequenceTree<ProgramState>;

impl From<crate::command::Sequence> for StateSequence {
	fn from(seq: crate::command::Sequence) -> Self {
		match seq {
			crate::command::Sequence::Run(program) => Self::Run(ProgramState::ToRun(program)),
			crate::command::Sequence::List(list) => {
				Self::List(list.into_iter().map(Self::from).collect())
			}
			crate::command::Sequence::Condition {
				given,
				then,
				otherwise,
			} => Self::Condition {
				given: Box::new(Self::from(*given)),
				then: then.map(|then| Box::new(Self::from(*then))),
				otherwise: otherwise.map(|otherwise| Box::new(Self::from(*otherwise))),
			},
		}
	}
}

impl StateSequence {
	pub(crate) fn current_child(&mut self) -> Option<&mut ErasedChild> {
		match self {
			Self::Run(ProgramState::IsRunning { child, .. }) => Some(child),
			Self::List(list) => list.iter_mut().find_map(|seq| seq.current_child()),
			Self::Condition {
				given,
				then,
				otherwise,
			} => given
				.current_child()
				.or_else(|| then.as_mut().and_then(|seq| seq.current_child()))
				.or_else(|| otherwise.as_mut().and_then(|seq| seq.current_child())),
			_ => None,
		}
	}

	// pub(crate) fn reset(&mut self) {
	// 	match self {
	// 		Self::Run(state) => *state = ProgramState::ToRun(state.program().clone()),
	// 		Self::List(list) => list.iter_mut().for_each(Self::reset),
	// 		Self::Condition {
	// 			given,
	// 			then,
	// 			otherwise,
	// 		} => {
	// 			given.reset();
	// 			if let Some(then) = then {
	// 				then.reset();
	// 			}
	// 			if let Some(otherwise) = otherwise {
	// 				otherwise.reset();
	// 			}
	// 		}
	// 	}
	// }
}
