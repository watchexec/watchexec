use std::{
	fs::File,
	io::{BufReader, Read},
	iter::once,
	sync::Arc,
};

use dashmap::DashMap;
use jaq_core::{CustomFilter, Definitions, Error, Val};
use miette::miette;
use tracing::{debug, error, info, trace, warn};

pub fn load_std_defs() -> miette::Result<Definitions> {
	debug!("loading jaq core library");
	let mut defs = Definitions::core();

	debug!("loading jaq standard library");
	let mut errs = Vec::new();
	jaq_std::std()
		.into_iter()
		.for_each(|def| defs.insert(def, &mut errs));

	if !errs.is_empty() {
		return Err(miette!("failed to load jaq standard library: {:?}", errs));
	}
	Ok(defs)
}

macro_rules! return_err {
	($err:expr) => {
		return Box::new(once($err))
	};
}

#[inline]
fn custom_err<T>(err: impl Into<String>) -> Result<T, Error> {
	Err(Error::Custom(err.into()))
}

macro_rules! string_arg {
	($args:expr, $n:expr, $ctx:expr, $val:expr) => {
		match $args[$n].run(($ctx.clone(), $val.clone())).next() {
			Some(Ok(Val::Str(v))) => Ok(v.to_string()),
			Some(Ok(val)) => custom_err(format!("expected string but got {val:?}")),
			Some(Err(e)) => Err(e),
			None => custom_err("value expected but none found"),
		}
	};
}

macro_rules! int_arg {
	($args:expr, $n:expr, $ctx:expr, $val:expr) => {
		match $args[$n].run(($ctx.clone(), $val.clone())).next() {
			Some(Ok(Val::Int(v))) => Ok(v as _),
			Some(Ok(val)) => custom_err(format!("expected int but got {val:?}")),
			Some(Err(e)) => Err(e),
			None => custom_err("value expected but none found"),
		}
	};
}

macro_rules! log_action {
	($level:expr, $val:expr) => {
		match $level.to_ascii_lowercase().as_str() {
			"trace" => trace!("jaq: {}", $val),
			"debug" => debug!("jaq: {}", $val),
			"info" => info!("jaq: {}", $val),
			"warn" => warn!("jaq: {}", $val),
			"error" => error!("jaq: {}", $val),
			_ => return_err!(custom_err("invalid log level")),
		}
	};
}

pub fn load_watchexec_defs(defs: &mut Definitions) -> miette::Result<()> {
	debug!("loading jaq watchexec library");

	trace!("jaq: add log filter");
	defs.insert_custom(
		"log",
		CustomFilter::with_update(
			1,
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

	trace!("jaq: add stdout filter");
	defs.insert_custom(
		"stdout",
		CustomFilter::with_update(
			0,
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

	trace!("jaq: add stderr filter");
	defs.insert_custom(
		"stderr",
		CustomFilter::with_update(
			0,
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

	let kv: Arc<DashMap<String, Val>> = Arc::new(DashMap::new());

	trace!("jaq: add kv_clear filter");
	defs.insert_custom(
		"kv_clear",
		CustomFilter::new(0, {
			let kv = kv.clone();
			move |_, (_, val)| {
				kv.clear();
				Box::new(once(Ok(val)))
			}
		}),
	);

	trace!("jaq: add kv_store filter");
	defs.insert_custom(
		"kv_store",
		CustomFilter::new(1, {
			let kv = kv.clone();
			move |args, (ctx, val)| {
				let key = match string_arg!(args, 0, ctx, val) {
					Ok(v) => v,
					Err(e) => return_err!(Err(e)),
				};

				kv.insert(key, val.clone());
				Box::new(once(Ok(val)))
			}
		}),
	);

	trace!("jaq: add kv_fetch filter");
	defs.insert_custom(
		"kv_fetch",
		CustomFilter::new(1, {
			move |args, (ctx, val)| {
				let key = match string_arg!(args, 0, ctx, val) {
					Ok(v) => v,
					Err(e) => return_err!(Err(e)),
				};

				Box::new(once(Ok(kv
					.get(&key)
					.map(|val| val.clone())
					.unwrap_or(Val::Null))))
			}
		}),
	);

	trace!("jaq: add read filter");
	defs.insert_custom(
		"read",
		CustomFilter::new(1, {
			move |args, (ctx, val)| {
				let path = match &val {
					Val::Str(v) => v.to_string(),
					_ => return_err!(custom_err("expected string (path) but got {val:?}")),
				};

				let bytes = match int_arg!(args, 0, ctx, &val) {
					Ok(v) => v,
					Err(e) => return_err!(Err(e)),
				};

				Box::new(once(Ok(match File::open(&path) {
					Ok(file) => {
						let buf_reader = BufReader::new(file);
						let mut limited = buf_reader.take(bytes);
						let mut buffer = String::with_capacity(bytes as _);
						match limited.read_to_string(&mut buffer) {
							Ok(read) => {
								debug!("jaq: read {read} bytes from {path:?}");
								Val::Str(buffer.into())
							}
							Err(err) => {
								error!("jaq: failed to read from {path:?}: {err:?}");
								Val::Null
							}
						}
					}
					Err(err) => {
						error!("jaq: failed to open file {path:?}: {err:?}");
						Val::Null
					}
				})))
			}
		}),
	);

	Ok(())
}
