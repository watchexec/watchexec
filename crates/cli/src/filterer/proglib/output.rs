use std::iter::once;

use jaq_interpret::{Error, Native, ParseCtx, Val};
use tracing::{debug, error, info, trace, warn};

use super::macros::*;

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

pub fn load(jaq: &mut ParseCtx) {
	trace!("jaq: add log filter");
	jaq.insert_native(
		"log".into(),
		1,
		Native::with_update(
			|args, (ctx, val)| {
				let level = match string_arg!(args, 0, ctx, val) {
					Ok(v) => v,
					Err(e) => return_err!(Err(e)),
				};

				log_action!(level, val);

				// passthrough
				Box::new(once(Ok(val)))
			},
			|args, (ctx, val), _| {
				let level = match string_arg!(args, 0, ctx, val) {
					Ok(v) => v,
					Err(e) => return_err!(Err(e)),
				};

				log_action!(level, val);

				// passthrough
				Box::new(once(Ok(val)))
			},
		),
	);

	trace!("jaq: add printout filter");
	jaq.insert_native(
		"printout".into(),
		0,
		Native::with_update(
			|_, (_, val)| {
				println!("{}", val);
				Box::new(once(Ok(val)))
			},
			|_, (_, val), _| {
				println!("{}", val);
				Box::new(once(Ok(val)))
			},
		),
	);

	trace!("jaq: add printerr filter");
	jaq.insert_native(
		"printerr".into(),
		0,
		Native::with_update(
			|_, (_, val)| {
				eprintln!("{}", val);
				Box::new(once(Ok(val)))
			},
			|_, (_, val), _| {
				eprintln!("{}", val);
				Box::new(once(Ok(val)))
			},
		),
	);
}
