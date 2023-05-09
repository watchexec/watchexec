//! Processor responsible for receiving events, filtering them, and scheduling actions in response.

#[doc(inline)]
pub use outcome::Outcome;
#[doc(inline)]
pub use worker::worker;
#[doc(inline)]
pub use workingdata::*;

mod outcome;
mod outcome_worker;
mod process_holder;
mod worker;
mod workingdata;
