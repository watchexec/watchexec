// listen-fd code inspired by systemdfd source by @mitsuhiko (Apache-2.0)
// https://github.com/mitsuhiko/systemfd/blob/master/src/fd.rs

#[cfg(unix)]
pub use std::os::fd::OwnedFd;
#[cfg(windows)]
pub use std::os::windows::io::OwnedSocket as OwnedFd;
use std::{
	ffi::OsStr,
	net::{IpAddr, Ipv4Addr, SocketAddr},
	num::{IntErrorKind, NonZero},
	str::FromStr,
};

use clap::{
	builder::TypedValueParser,
	error::{Error, ErrorKind},
	ValueEnum,
};
use miette::{IntoDiagnostic, Result};

#[cfg(test)]
#[path = "fd_socket_test.rs"]
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

#[derive(Clone)]
pub(crate) struct FdSpecValueParser;

impl TypedValueParser for FdSpecValueParser {
	type Value = FdSpec;

	fn parse_ref(
		&self,
		_cmd: &clap::Command,
		_arg: Option<&clap::Arg>,
		value: &OsStr,
	) -> Result<Self::Value, Error> {
		let value = value
			.to_str()
			.ok_or_else(|| Error::raw(ErrorKind::ValueValidation, "invalid UTF-8"))?
			.to_ascii_lowercase();

		let (socket, value) = if let Some(val) = value.strip_prefix("tcp::") {
			(SocketType::Tcp, val)
		} else if let Some(val) = value.strip_prefix("udp::") {
			(SocketType::Udp, val)
		} else if let Some((pre, _)) = value.split_once("::") {
			if !pre.starts_with("[") {
				return Err(Error::raw(
					ErrorKind::ValueValidation,
					format!("invalid prefix {pre:?}"),
				));
			}

			(SocketType::Tcp, value.as_ref())
		} else {
			(SocketType::Tcp, value.as_ref())
		};

		let addr = if let Ok(addr) = SocketAddr::from_str(value) {
			addr
		} else {
			match NonZero::<u16>::from_str(value) {
				Ok(port) => SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port.get()),
				Err(err) if *err.kind() == IntErrorKind::Zero => {
					return Err(Error::raw(
						ErrorKind::ValueValidation,
						"invalid port number: cannot be zero",
					))
				}
				Err(err) if *err.kind() == IntErrorKind::PosOverflow => {
					return Err(Error::raw(
						ErrorKind::ValueValidation,
						"invalid port number: greater than 65535",
					))
				}
				Err(_) => {
					return Err(Error::raw(
						ErrorKind::ValueValidation,
						"invalid port number",
					))
				}
			}
		};

		Ok(FdSpec { socket, addr })
	}
}

impl FdSpec {
	pub fn create_fd(&self) -> Result<OwnedFd> {
		#[cfg(not(any(unix, windows)))]
		{
			miette::bail!("--fd-socket not supported on this platform");
		}

		#[cfg(unix)]
		{
			self.create_fd_imp().into_diagnostic()
		}

		#[cfg(windows)]
		{
			self.create_fd_imp()
		}
	}

	#[cfg(unix)]
	fn create_fd_imp(&self) -> nix::Result<OwnedFd> {
		use std::os::fd::AsRawFd;

		use nix::sys::socket::{
			bind, listen, setsockopt, socket, sockopt, AddressFamily, Backlog, SockFlag, SockType,
			SockaddrStorage,
		};

		let addr = SockaddrStorage::from(self.addr);
		let fam = if self.addr.is_ipv4() {
			AddressFamily::Inet
		} else {
			AddressFamily::Inet6
		};
		let ty = match self.socket {
			SocketType::Tcp => SockType::Stream,
			SocketType::Udp => SockType::Datagram,
		};

		let sock = socket(fam, ty, SockFlag::empty(), None)?;

		setsockopt(&sock, sockopt::ReuseAddr, &true)?;
		setsockopt(&sock, sockopt::ReusePort, &true)?;

		let rv = bind(sock.as_raw_fd(), &addr)
			.map_err(From::from)
			.and_then(|_| {
				if let SocketType::Tcp = self.socket {
					listen(&sock, Backlog::new(1).unwrap())?;
				}
				Ok(())
			});

		if rv.is_err() {
			unsafe { libc::close(sock.as_raw_fd()) };
		}

		rv.map(|_| sock)
	}

	#[cfg(windows)]
	fn create_fd_imp(&self) -> Result<OwnedFd> {
		use socket2::{Domain, SockAddr, Socket, Type};

		let addr = SockAddr::from(self.addr);
		let dom = if self.addr.is_ipv4() {
			Domain::IPV4
		} else {
			Domain::IPV6
		};
		let ty = match self.socket {
			SocketType::Tcp => Type::STREAM,
			SocketType::Udp => Type::DGRAM,
		};

		let sock = Socket::new(dom, ty, None).into_diagnostic()?;
		sock.set_reuse_address(true).into_diagnostic()?;
		sock.bind(&addr).into_diagnostic()?;

		if let SocketType::Tcp = self.socket {
			sock.listen(1).into_diagnostic()?;
		}

		Ok(sock.into())
	}
}
