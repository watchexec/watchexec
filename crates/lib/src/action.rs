//! Processor responsible for receiving events, filtering them, and scheduling actions in response.

#[doc(inline)]
pub use handler::Handler as ActionHandler;
#[doc(inline)]
pub use quit::QuitManner;
#[doc(inline)]
pub use r#return::ActionReturn;
#[doc(inline)]
pub use worker::worker;

mod handler;
mod quit;
mod r#return;
mod worker;
