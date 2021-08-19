// it would be good to have an async handle here but it's not clear how to do it

/// A callable that can be used to hook into watchexec.
pub trait Handler<T> {
	type Error: std::error::Error;

	fn handle(&mut self, _data: T) -> Result<(), Self::Error>;
}

impl<F, T, E> Handler<T> for F
where
	E: std::error::Error,
	F: FnMut(T) -> Result<(), E> + Send + 'static,
{
	type Error = E;

	fn handle(&mut self, data: T) -> Result<(), Self::Error> {
		(self)(data)
	}
}

impl<T> Handler<T> for std::sync::mpsc::Sender<T>
where
	T: Send,
{
	type Error = std::sync::mpsc::SendError<T>;

	fn handle(&mut self, data: T) -> Result<(), Self::Error> {
		self.send(data)
	}
}

impl<T> Handler<T> for tokio::sync::mpsc::Sender<T>
where
	T: std::fmt::Debug,
{
	type Error = tokio::sync::mpsc::error::TrySendError<T>;

	fn handle(&mut self, data: T) -> Result<(), Self::Error> {
		self.try_send(data)
	}
}
