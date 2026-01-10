use chumsky::{
	extension::v1::{Ext, ExtParser},
	input::{Checkpoint, Cursor, InputRef},
	inspector::Inspector,
	prelude::*,
	text::newline,
};
use tracing::trace;

pub type ParserErr<'src> =
	chumsky::extra::Full<chumsky::error::Rich<'src, char>, LogInspector<char>, ()>;

#[derive(Clone, Debug)]
pub struct DebugLabel_<A> {
	parser: A,
	label: &'static str,
}

pub type DebugLabel<A> = Ext<DebugLabel_<A>>;

pub trait ParserDebugExt<'src, I, O, E>
where
	I: Input<'src>,
	E: extra::ParserExtra<'src, I>,
{
	fn debug(self, label: &'static str) -> DebugLabel<Self>
	where
		Self: Sized,
	{
		Ext(DebugLabel_ {
			parser: self,
			label,
		})
	}
}

impl<'src, P, I, O, E> ParserDebugExt<'src, I, O, E> for P
where
	P: Parser<'src, I, O, E>,
	I: Input<'src>,
	E: extra::ParserExtra<'src, I>,
{
}

impl<'src, A, I, O, E> ExtParser<'src, I, O, E> for DebugLabel_<A>
where
	A: Parser<'src, I, O, E>,
	I: Input<'src>,
	E: extra::ParserExtra<'src, I>,
{
	fn parse(&self, inp: &mut InputRef<'src, '_, I, E>) -> Result<O, E::Error> {
		trace!("entered parser {}", self.label);
		inp.parse(&self.parser)
	}

	fn check(&self, inp: &mut InputRef<'src, '_, I, E>) -> Result<(), E::Error> {
		trace!("entered checker {}", self.label);
		inp.check(&self.parser)
	}
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LogInspector<T>(pub T);

impl<'src, T, I: Input<'src>> Inspector<'src, I> for LogInspector<T>
where
	<I as Input<'src>>::Token: std::fmt::Debug,
	<I as Input<'src>>::Cursor: std::fmt::Debug,
{
	type Checkpoint = ();

	#[inline(always)]
	fn on_token(&mut self, token: &<I as Input<'src>>::Token) {
		trace!("read token {:?}", token);
	}

	#[inline(always)]
	fn on_save<'parse>(&self, _: &Cursor<'src, 'parse, I>) -> Self::Checkpoint {}

	#[inline(always)]
	fn on_rewind<'parse>(&mut self, checkpoint: &Checkpoint<'src, 'parse, I, Self::Checkpoint>) {
		trace!("rewound to {:?}", checkpoint.cursor().inner());
	}
}

pub fn any_nonl<'src>() -> impl Parser<'src, &'src str, char, ParserErr<'src>> + Clone {
	any().and_is(newline().not()).debug("any")
}

pub fn none_of_nonl<'src>(
	none: &'src str,
) -> impl Parser<'src, &'src str, char, ParserErr<'src>> + Clone {
	any()
		.and_is(newline().or(one_of(none).to(())).not())
		.debug("none_of")
}
