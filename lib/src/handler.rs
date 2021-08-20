//! Trait and implementations for hook handlers.
//!
//! You can implement the trait yourself, or use any of the provided implementations:
//! - for closures,
//! - for std and tokio channels,
//! - for printing to writers, in `Debug` and `Display` (where supported) modes (generally used for
//!   debugging and testing, as they don't allow any other output customisation).
//!
//! The implementation for [`FnMut`] only supports fns that return a [`Future`]. Unfortunately
//! it's not possible to provide an implementation for fns that don't return a `Future` as well,
//! so to call sync code you must either provide an async handler, or use the [`SyncFnHandler`]
//! wrapper.
//!
//! # Examples
//!
//! In each example `on_data` is the following function:
//!
//! ```
//! # use watchexec::handler::Handler;
//! fn on_data<T: Handler<Vec<u8>>>(_: T) {}
//! ```
//!
//! Async closure:
//!
//! ```
//! use tokio::io::{AsyncWriteExt, stdout};
//! # use watchexec::handler::Handler;
//! # fn on_data<T: Handler<Vec<u8>>>(_: T) {}
//! on_data(|data: Vec<u8>| async move {
//!     stdout().write_all(&data).await
//! });
//! ```
//!
//! Sync code in async closure:
//!
//! ```
//! use std::io::{Write, stdout};
//! # use watchexec::handler::Handler;
//! # fn on_data<T: Handler<Vec<u8>>>(_: T) {}
//! on_data(|data: Vec<u8>| async move {
//!     stdout().write_all(&data)
//! });
//! ```
//!
//! Sync closure with wrapper:
//!
//! ```
//! use std::io::{Write, stdout};
//! # use watchexec::handler::{Handler, SyncFnHandler};
//! # fn on_data<T: Handler<Vec<u8>>>(_: T) {}
//! on_data(SyncFnHandler::from(|data: Vec<u8>| {
//!     stdout().write_all(&data)
//! }));
//! ```
//!
//! Std channel:
//!
//! ```
//! use std::sync::mpsc;
//! # use watchexec::handler::Handler;
//! # fn on_data<T: Handler<Vec<u8>>>(_: T) {}
//! let (s, r) = mpsc::channel();
//! on_data(s);
//! ```
//!
//! Tokio channel:
//!
//! ```
//! use tokio::sync::mpsc;
//! # use watchexec::handler::Handler;
//! # fn on_data<T: Handler<Vec<u8>>>(_: T) {}
//! let (s, r) = mpsc::channel(123);
//! on_data(s);
//! ```
//!
//! Printing to console:
//!
//! ```
//! use std::io::{Write, stderr, stdout};
//! # use watchexec::handler::{Handler, PrintDebug, PrintDisplay};
//! # fn on_data<T: Handler<String>>(_: T) {}
//! on_data(PrintDebug(stdout()));
//! on_data(PrintDisplay(stderr()));
//! ```

use std::{error::Error, future::Future, io::Write, marker::PhantomData};

use tokio::runtime::Handle;
use tracing::{event, Level};

/// A callable that can be used to hook into watchexec.
pub trait Handler<T> {
	/// Call the handler with the given data.
	fn handle(&mut self, _data: T) -> Result<(), Box<dyn Error>>;
}

/// Wrapper for [`Handler`]s that are non-future [`FnMut`]s.
///
/// Construct using [`Into::into`]:
///
/// ```
/// # use watchexec::handler::{Handler as _, SyncFnHandler};
/// # let f: SyncFnHandler<(), std::io::Error, _> =
/// (|data| { dbg!(data); Ok(()) }).into()
/// # ;
/// ```
///
/// or [`From::from`]:
///
/// ```
/// # use watchexec::handler::{Handler as _, SyncFnHandler};
/// # let f: SyncFnHandler<(), std::io::Error, _> =
/// SyncFnHandler::from(|data| { dbg!(data); Ok(()) });
/// ```
pub struct SyncFnHandler<T, E, F>
where
	E: Error + 'static,
	F: FnMut(T) -> Result<(), E> + Send + 'static,
{
	inner: F,
	_t: PhantomData<T>,
	_e: PhantomData<E>,
}

impl<T, E, F> From<F> for SyncFnHandler<T, E, F>
where
	E: Error + 'static,
	F: FnMut(T) -> Result<(), E> + Send + 'static,
{
	fn from(inner: F) -> Self {
		Self {
			inner,
			_t: PhantomData,
			_e: PhantomData,
		}
	}
}

impl<T, E, F> Handler<T> for SyncFnHandler<T, E, F>
where
	E: Error + 'static,
	F: FnMut(T) -> Result<(), E> + Send + 'static,
{
	fn handle(&mut self, data: T) -> Result<(), Box<dyn Error>> {
		(self.inner)(data).map_err(|e| Box::new(e) as _)
	}
}

impl<F, U, T, E> Handler<T> for F
where
	E: Error + 'static,
	F: FnMut(T) -> U + Send + 'static,
	U: Future<Output = Result<(), E>>,
{
	fn handle(&mut self, data: T) -> Result<(), Box<dyn Error>> {
		// this will always be called within watchexec context, which runs within tokio
		Handle::current()
			.block_on((self)(data))
			.map_err(|e| Box::new(e) as _)
	}
}

impl<T> Handler<T> for std::sync::mpsc::Sender<T>
where
	T: Send + 'static,
{
	fn handle(&mut self, data: T) -> Result<(), Box<dyn Error>> {
		self.send(data).map_err(|e| Box::new(e) as _)
	}
}

impl<T> Handler<T> for tokio::sync::mpsc::Sender<T>
where
	T: std::fmt::Debug + 'static,
{
	fn handle(&mut self, data: T) -> Result<(), Box<dyn Error>> {
		self.try_send(data).map_err(|e| Box::new(e) as _)
	}
}

/// A handler implementation to print to any [`Write`]r (e.g. stdout) in `Debug` format.
pub struct PrintDebug<W: Write>(pub W);

impl<T, W> Handler<T> for PrintDebug<W>
where
	T: std::fmt::Debug,
	W: Write,
{
	fn handle(&mut self, data: T) -> Result<(), Box<dyn Error>> {
		writeln!(self.0, "{:?}", data).map_err(|e| Box::new(e) as _)
	}
}

/// A handler implementation to print to any [`Write`]r (e.g. stdout) in `Display` format.
pub struct PrintDisplay<W: Write>(pub W);

impl<T, W> Handler<T> for PrintDisplay<W>
where
	T: std::fmt::Display,
	W: Write,
{
	fn handle(&mut self, data: T) -> Result<(), Box<dyn Error>> {
		writeln!(self.0, "{}", data).map_err(|e| Box::new(e) as _)
	}
}
