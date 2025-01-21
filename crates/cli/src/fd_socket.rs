// listen-fd code inspired by systemdfd source by @mitsuhiko (Apache-2.0)
// https://github.com/mitsuhiko/systemfd/blob/master/src/fd.rs

use std::net::SocketAddr;

use clap::ValueEnum;
use miette::Result;

pub(crate) use imp::*;
pub(crate) use parser::FdSpecValueParser;

#[cfg(unix)]
#[path = "fd_socket/unix.rs"]
mod imp;
#[cfg(windows)]
#[path = "fd_socket/windows.rs"]
mod imp;
#[cfg(not(any(unix, windows)))]
#[path = "fd_socket/fallback.rs"]
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
pub struct FdSpec {
	pub socket: SocketType,
	pub addr: SocketAddr,
}

pub(crate) trait Sockets
where
	Self: Sized,
{
	async fn create(specs: &[FdSpec]) -> Result<Self>;
	fn envs(&self) -> Vec<(&'static str, String)>;
	fn serve(&mut self) {}
}
