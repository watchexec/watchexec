//! Processor responsible for receiving events, filtering them, and scheduling actions in response.

#[doc(inline)]
pub use handler::*;
#[doc(inline)]
pub use outcome::Outcome;
#[doc(inline)]
pub use worker::worker;

mod handler;
mod outcome;
mod outcome_worker;
mod process_holder;
mod worker;
