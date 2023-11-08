use std::sync::atomic::{AtomicUsize, Ordering};

static JOB_NEXT: AtomicUsize = AtomicUsize::new(1);
static TICKET_NEXT: AtomicUsize = AtomicUsize::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct JobId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TicketId(u64);

impl Default for JobId {
	fn default() -> Self {
		Self(JOB_NEXT.fetch_add(1, Ordering::Relaxed) as u64)
	}
}

impl Default for TicketId {
	fn default() -> Self {
		Self(TICKET_NEXT.fetch_add(1, Ordering::Relaxed) as u64)
	}
}
