use std::{cell::Cell, num::NonZeroU64};

/// Unique opaque identifier.
#[must_use]
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub struct Id {
	thread: NonZeroU64,
	counter: u64,
}

thread_local! {
	static COUNTER: Cell<u64> = const { Cell::new(0) };
}

impl Default for Id {
	fn default() -> Self {
		let counter = COUNTER.get();
		COUNTER.set(counter.wrapping_add(1));

		Self {
			thread: threadid(),
			counter,
		}
	}
}

fn threadid() -> NonZeroU64 {
	use std::hash::{Hash, Hasher};

	struct Extractor {
		id: u64,
	}

	impl Hasher for Extractor {
		fn finish(&self) -> u64 {
			self.id
		}

		fn write(&mut self, _bytes: &[u8]) {}
		fn write_u64(&mut self, n: u64) {
			self.id = n;
		}
	}

	let mut ex = Extractor { id: 0 };
	std::thread::current().id().hash(&mut ex);

	// SAFETY: guaranteed to be > 0
	// safeguarded by the max(1), but this is already guaranteed by the thread id being a NonZeroU64
	// internally; as that guarantee is not stable, we do make sure, just to be on the safe side.
	unsafe { NonZeroU64::new_unchecked(ex.finish().max(1)) }
}

// Replace with this when the thread_id_value feature is stable
// fn threadid() -> NonZeroU64 {
// 	std::thread::current().id().as_u64()
// }

#[test]
fn test_threadid() {
	let top = threadid();
	std::thread::spawn(move || {
		assert_ne!(top, threadid());
	})
	.join()
	.expect("thread failed");
}
