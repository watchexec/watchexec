//! Processor responsible for receiving events, filtering them, and scheduling actions in response.

#[doc(inline)]
pub use outcome::Outcome;
#[doc(inline)]
pub use worker::{throttle_collect, worker};
#[doc(inline)]
pub use workingdata::*;

mod outcome;
pub mod outcome_worker;
pub mod process_holder;
mod worker;
mod workingdata;
