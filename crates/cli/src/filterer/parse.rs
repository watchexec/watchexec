use std::{fmt::Debug, path::PathBuf};

use jaq_core::{
	load::{Arena, File, Loader},
	Ctx, Filter, Native, RcIter,
};
use jaq_json::Val;
use miette::{miette, IntoDiagnostic, Result, WrapErr};
use tokio::io::AsyncReadExt;
use tracing::{debug, trace};
use watchexec_events::Event;

use super::proglib::jaq_lib;

#[derive(Clone)]
pub enum FilterProgram {
	Jaq(Filter<Native<Val>>),
}

impl Debug for FilterProgram {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Jaq(_) => f.debug_tuple("Jaq").field(&"filter").finish(),
		}
	}
}

impl FilterProgram {
	pub(crate) async fn new_jaq_from_file(path: impl Into<PathBuf>) -> Result<Self> {
		async fn inner(path: PathBuf) -> Result<FilterProgram> {
			trace!(?path, "reading filter program from file");
			let mut progfile = tokio::fs::File::open(&path).await.into_diagnostic()?;
			let mut buf =
				String::with_capacity(progfile.metadata().await.into_diagnostic()?.len() as _);
			let bytes_read = progfile.read_to_string(&mut buf).await.into_diagnostic()?;
			debug!(?path, %bytes_read, "read filter program from file");
			FilterProgram::new_jaq(path, buf)
		}

		let path = path.into();
		let error = format!("in file {path:?}");
		inner(path).await.wrap_err(error)
	}

	pub(crate) fn new_jaq_from_arg(n: usize, arg: String) -> Result<Self> {
		let path = PathBuf::from(format!("<arg {n}>"));
		let error = format!("in --filter-prog {n}");
		Self::new_jaq(path, arg).wrap_err(error)
	}

	fn new_jaq(path: PathBuf, code: String) -> Result<Self> {
		let user_lib_paths = [
			PathBuf::from("~/.jq"),
			PathBuf::from("$ORIGIN/../lib/jq"),
			PathBuf::from("$ORIGIN/../lib"),
		];
		let arena = Arena::default();
		let loader =
			Loader::new(jaq_std::defs().chain(jaq_json::defs())).with_std_read(&user_lib_paths);
		let modules = match loader.load(&arena, File { path, code: &code }) {
			Ok(m) => m,
			Err(errs) => {
				let errs = errs
					.into_iter()
					.map(|(_, err)| format!("{err:?}"))
					.collect::<Vec<_>>()
					.join("\n");
				return Err(miette!("{}", errs).wrap_err("failed to load filter program"));
			}
		};

		let filter = jaq_lib()
			.compile(modules)
			.map_err(|errs| miette!("Failed to compile jaq program: {:?}", errs))?;
		Ok(Self::Jaq(filter))
	}

	pub(crate) fn run(&self, event: &Event) -> Result<bool> {
		match self {
			Self::Jaq(filter) => {
				let inputs = RcIter::new(std::iter::empty());
				let val = serde_json::to_value(event)
					.map_err(|err| miette!("failed to serialize event: {}", err))
					.map(Val::from)?;

				let mut results = filter.run((Ctx::new([], &inputs), val));
				results
					.next()
					.ok_or_else(|| miette!("returned no value"))?
					.map_err(|err| miette!("program failed: {err}"))
					.and_then(|val| match val {
						Val::Bool(b) => Ok(b),
						val => Err(miette!("returned non-boolean {val:?}")),
					})
			}
		}
	}
}
