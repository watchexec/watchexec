#[doc(inline)]
pub use self::{
	job::Job,
	messages::{Control, Ticket},
	priority::Priority,
	state::CommandState,
	task::JobTaskContext,
};

#[doc(inline)]
pub use task::start_job;

#[cfg(test)]
pub use testchild::{TestChild, TestChildCall};

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
