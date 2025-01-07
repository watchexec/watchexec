use jaq_core::{Compiler, Native};

mod file;
mod hash;
mod kv;
mod macros;
mod output;

pub fn jaq_lib<'s>() -> Compiler<&'s str, Native<jaq_json::Val>> {
	Compiler::<_, Native<_>>::default().with_funs(
		jaq_std::funs()
			.chain(jaq_json::funs())
			.chain(file::funs())
			.chain(hash::funs())
			.chain(kv::funs())
			.chain(output::funs()),
	)
}
