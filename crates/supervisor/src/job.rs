//! Job supervision.

#[doc(inline)]
pub use self::{
	job::Job,
	messages::{Control, Ticket},
	state::CommandState,
	task::JobTaskContext,
};

#[cfg(test)]
pub(crate) use self::{
	priority::Priority,
	testchild::{TestChild, TestChildCall},
};

#[doc(inline)]
pub use task::start_job;

#[allow(clippy::module_inception)]
mod job;
mod messages;
mod priority;
mod state;
mod task;

#[cfg(test)]
mod testchild;

#[cfg(test)]
mod test;
