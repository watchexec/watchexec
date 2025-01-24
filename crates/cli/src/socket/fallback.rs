use miette::{bail, Result};

use crate::args::command::EnvVar;

use super::{SocketSpec, Sockets};

#[derive(Debug)]
pub struct SocketSet;

impl SocketSet for SocketSet {
	async fn create(_: &[SocketSpec]) -> Result<Self> {
		bail!("--socket is not supported on your platform")
	}

	fn envs(&self) -> Vec<EnvVar> {
		Vec::new()
	}
}
