use std::future::Future;

use futures::{stream::FuturesUnordered, StreamExt};
use tokio::task::{JoinError, JoinHandle};

/// A collection of tasks spawned on a Tokio runtime.
///
/// This is a variant of Tokio's [`JoinSet`](tokio::task::JoinSet) which exposes the inner `insert`
/// method, such that it can be given tasks after they've been spawned. As a simplification, the
/// return type of the tasks is fixed to `()`.
///
/// The inner implementation skips the `IdleNotifiedSet` and uses two `Vec`s instead. This is
/// probably not a great idea but it does work.
///
/// # Examples
///
/// Spawn multiple tasks and wait for them.
///
/// ```
/// use watchexec::LateJoinSet;
///
/// #[tokio::main]
/// async fn main() {
///     let mut set = LateJoinSet::default();
///
///     for i in 0..10 {
///         set.spawn(async move { println!("{i}"); });
///     }
///
///     let mut seen = [false; 10];
///     while let Some(res) = set.join_next().await {
///         let idx = res.unwrap();
///         seen[idx] = true;
///     }
///
///     for i in 0..10 {
///         assert!(seen[i]);
///     }
/// }
/// ```
///
/// Attach a task to a set after it's been spawned.
///
/// ```
/// use watchexec::LateJoinSet;
///
/// #[tokio::main]
/// async fn main() {
///     let mut set = LateJoinSet::default();
///
///     let handle = tokio::spawn(async move { println!("Hello, world!"); });
///     set.insert(handle);
///     set.abort_all();
/// }
/// ```
#[derive(Debug, Default)]
pub struct LateJoinSet {
	tasks: FuturesUnordered<JoinHandle<()>>,
}

impl LateJoinSet {
	/// Spawn the provided task on the `LateJoinSet`.
	///
	/// The provided future will start running in the background immediately when this method is
	/// called, even if you don't await anything on this `LateJoinSet`.
	///
	/// # Panics
	///
	/// This method panics if called outside of a Tokio runtime.
	#[track_caller]
	pub fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) {
		self.insert(tokio::spawn(task));
	}

	/// Insert an already-spawned task into the [`LateJoinSet`].
	pub fn insert(&self, task: JoinHandle<()>) {
		self.tasks.push(task);
	}

	/// Waits until one of the tasks in the set completes.
	///
	/// Returns `None` if the set is empty.
	pub async fn join_next(&mut self) -> Option<Result<(), JoinError>> {
		self.tasks.next().await
	}

	/// Waits until all the tasks in the set complete.
	///
	/// Ignores any panics in the tasks shutting down.
	pub async fn join_all(&mut self) {
		while self.join_next().await.is_some() {}
		self.tasks.clear();
	}

	/// Aborts all tasks on this `LateJoinSet`.
	///
	/// This does not remove the tasks from the `LateJoinSet`. To wait for the tasks to complete
	/// cancellation, use `join_all` or call `join_next` in a loop until the `LateJoinSet` is empty.
	pub fn abort_all(&self) {
		self.tasks.iter().for_each(|jh| jh.abort());
	}
}

impl Drop for LateJoinSet {
	fn drop(&mut self) {
		self.abort_all();
		self.tasks.clear();
	}
}
