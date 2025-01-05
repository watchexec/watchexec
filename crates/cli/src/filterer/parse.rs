use miette::{miette, Result};

pub fn parse_filter_program((n, prog): (usize, String)) -> Result<jaq_syn::Main> {
	let parser = jaq_parse::main();
	let (main, errs) = jaq_parse::parse(&prog, parser);

	if !errs.is_empty() {
		let errs = errs
			.into_iter()
			.map(|err| err.to_string())
			.collect::<Vec<_>>()
			.join("\n");
		return Err(miette!("{}", errs).wrap_err(format!("failed to load filter program #{n}")));
	}

	main.ok_or_else(|| miette!("failed to load filter program #{} (no reason given)", n))
}
