macro_rules! return_err {
	($err:expr) => {
		return Box::new(once($err.map_err(Into::into)))
	};
}
pub(crate) use return_err;
