use std::iter::once;

use jaq_core::{Ctx, Error, Native};
use jaq_json::Val;
use jaq_std::{v, Filter};
use tracing::{debug, error, info, trace, warn};

use super::macros::return_err;

macro_rules! log_action {
	($level:expr, $val:expr) => {
		match $level.to_ascii_lowercase().as_str() {
			"trace" => trace!("jaq: {}", $val),
			"debug" => debug!("jaq: {}", $val),
			"info" => info!("jaq: {}", $val),
			"warn" => warn!("jaq: {}", $val),
			"error" => error!("jaq: {}", $val),
			_ => return_err!(Err(Error::str("invalid log level"))),
		}
	};
}

pub fn funs() -> [Filter<Native<jaq_json::Val>>; 3] {
	[
		(
			"log",
			v(1),
			Native::new(|_, (mut ctx, val): (Ctx<'_, Val>, _)| {
				let level = ctx.pop_var().to_string();
				log_action!(level, val);

				// passthrough
				Box::new(once(Ok(val)))
			})
			.with_update(|_, (mut ctx, val), _| {
				let level = ctx.pop_var().to_string();
				log_action!(level, val);

				// passthrough
				Box::new(once(Ok(val)))
			}),
		),
		(
			"printout",
			v(0),
			Native::new(|_, (_, val)| {
				println!("{val}");
				Box::new(once(Ok(val)))
			})
			.with_update(|_, (_, val), _| {
				println!("{val}");
				Box::new(once(Ok(val)))
			}),
		),
		(
			"printerr",
			v(0),
			Native::new(|_, (_, val)| {
				eprintln!("{val}");
				Box::new(once(Ok(val)))
			})
			.with_update(|_, (_, val), _| {
				eprintln!("{val}");
				Box::new(once(Ok(val)))
			}),
		),
	]
}
