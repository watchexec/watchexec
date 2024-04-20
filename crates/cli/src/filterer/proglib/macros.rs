macro_rules! return_err {
	($err:expr) => {
		return Box::new(once($err))
	};
}
pub(crate) use return_err;

macro_rules! string_arg {
	($args:expr, $n:expr, $ctx:expr, $val:expr) => {
		match ::jaq_interpret::FilterT::run($args.get($n), ($ctx.clone(), $val.clone())).next() {
			Some(Ok(Val::Str(v))) => Ok(v.to_string()),
			Some(Ok(val)) => Err(Error::str(format!("expected string but got {val:?}"))),
			Some(Err(e)) => Err(e),
			None => Err(Error::str("value expected but none found")),
		}
	};
}
pub(crate) use string_arg;

macro_rules! int_arg {
	($args:expr, $n:expr, $ctx:expr, $val:expr) => {
		match ::jaq_interpret::FilterT::run($args.get($n), ($ctx.clone(), $val.clone())).next() {
			Some(Ok(Val::Int(v))) => Ok(v as _),
			Some(Ok(val)) => Err(Error::str(format!("expected int but got {val:?}"))),
			Some(Err(e)) => Err(e),
			None => Err(Error::str("value expected but none found")),
		}
	};
}
pub(crate) use int_arg;
