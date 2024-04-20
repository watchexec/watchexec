use jaq_interpret::ParseCtx;
use miette::Result;
use tracing::debug;

mod file;
mod hash;
mod kv;
mod macros;
mod output;

pub fn jaq_lib() -> Result<ParseCtx> {
	let mut jaq = ParseCtx::new(Vec::new());

	debug!("loading jaq core library");
	jaq.insert_natives(jaq_core::core());

	debug!("loading jaq std library");
	jaq.insert_defs(jaq_std::std());

	debug!("loading jaq watchexec library");
	file::load(&mut jaq);
	hash::load(&mut jaq);
	kv::load(&mut jaq);
	output::load(&mut jaq);

	Ok(jaq)
}
