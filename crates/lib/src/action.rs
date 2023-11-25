//! Processor responsible for receiving events, filtering them, and scheduling actions in response.

#[doc(inline)]
pub use handler::*;
#[doc(inline)]
pub use worker::worker;

mod handler;
mod worker;
