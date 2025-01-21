use miette::{bail, Result};

use crate::args::command::EnvVar;

use super::{FdSpec, Sockets};

#[derive(Debug)]
pub struct FdSockets;

impl Sockets for FdSockets {
	async fn create(_: &[FdSpec]) -> Result<Self> {
		bail!("--fd-socket is not supported on your platform")
	}

	fn envs(&self) -> impl Iterator<Item = EnvVar> {
		std::iter::empty()
	}
}
