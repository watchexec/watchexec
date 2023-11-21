//! A flag that can be raised to wake a task.
//!
//! Copied wholesale from <https://docs.rs/futures/latest/futures/task/struct.AtomicWaker.html>
//! unfortunately not aware of crated version!

use std::{
	pin::Pin,
	sync::{
		atomic::{AtomicBool, Ordering::Relaxed},
		Arc,
	},
};

use futures::{
	future::Future,
	task::{AtomicWaker, Context, Poll},
};

#[derive(Debug)]
struct Inner {
	waker: AtomicWaker,
	set: AtomicBool,
}

#[derive(Clone, Debug)]
pub struct Flag(Arc<Inner>);

impl Default for Flag {
	fn default() -> Self {
		Self::new(false)
	}
}

impl Flag {
	pub fn new(value: bool) -> Self {
		Self(Arc::new(Inner {
			waker: AtomicWaker::new(),
			set: AtomicBool::new(value),
		}))
	}

	pub fn raised(&self) -> bool {
		self.0.set.load(Relaxed)
	}

	pub fn raise(&self) {
		self.0.set.store(true, Relaxed);
		self.0.waker.wake();
	}
}

impl Future for Flag {
	type Output = ();

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
		// quick check to avoid registration if already done.
		if self.0.set.load(Relaxed) {
			return Poll::Ready(());
		}

		self.0.waker.register(cx.waker());

		// Need to check condition **after** `register` to avoid a race
		// condition that would result in lost notifications.
		if self.0.set.load(Relaxed) {
			Poll::Ready(())
		} else {
			Poll::Pending
		}
	}
}
