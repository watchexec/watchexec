// listen-fd code inspired by systemdfd source by @mitsuhiko (Apache-2.0)
// https://github.com/mitsuhiko/systemfd/blob/master/src/fd.rs

use std::net::SocketAddr;

use clap::ValueEnum;
use miette::Result;

pub(crate) use imp::*;
pub(crate) use parser::SocketSpecValueParser;

use crate::args::command::EnvVar;

#[cfg(unix)]
#[path = "socket/unix.rs"]
mod imp;
#[cfg(windows)]
#[path = "socket/windows.rs"]
mod imp;
#[cfg(not(any(unix, windows)))]
#[path = "socket/fallback.rs"]
mod imp;
mod parser;
#[cfg(test)]
mod test;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum SocketType {
	#[default]
	Tcp,
	Udp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SocketSpec {
	pub socket: SocketType,
	pub addr: SocketAddr,
}

pub(crate) trait Sockets
where
	Self: Sized,
{
	async fn create(specs: &[SocketSpec]) -> Result<Self>;
	fn envs(&self, pid: u32) -> impl Iterator<Item = EnvVar>;
	fn serve(&mut self) {}
}
