#[doc(inline)]
pub use self::{
	job::Job,
	messages::{Control, Ticket},
	priority::Priority,
	state::CommandState,
	task::JobTaskContext,
};

pub use task::start_job; // TODO: remove pub (dev only)

#[allow(clippy::module_inception)]
mod job;
mod messages;
mod priority;
mod state;
mod task;
