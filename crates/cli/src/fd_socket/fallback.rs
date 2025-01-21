use miette::{bail, Result};

use super::{FdSpec, Sockets};

#[derive(Debug)]
pub struct FdSockets;

impl Sockets for FdSockets {
	async fn create(_: &[FdSpec]) -> Result<Self> {
		bail!("--fd-socket is not supported on your platform")
	}

	fn envs(&self) -> Vec<(&'static str, String)> {
		Vec::new()
	}
}
