use std::os::fd::{AsRawFd, OwnedFd};

use miette::{IntoDiagnostic, Result};
use nix::sys::socket::{
	bind, listen, setsockopt, socket, sockopt, AddressFamily, Backlog, SockFlag, SockType,
	SockaddrStorage,
};
use tracing::instrument;

use crate::args::command::EnvVar;

use super::{SocketSpec, SocketType, Sockets};

#[derive(Debug)]
pub struct SocketSet {
	fds: Vec<OwnedFd>,
}

impl Sockets for SocketSet {
	#[instrument(level = "trace")]
	async fn create(specs: &[SocketSpec]) -> Result<Self> {
		debug_assert!(!specs.is_empty());
		specs
			.into_iter()
			.map(SocketSpec::create)
			.collect::<Result<Vec<_>>>()
			.map(|fds| Self { fds })
	}

	#[instrument(level = "trace")]
	fn envs(&self) -> Vec<EnvVar> {
		vec![
			EnvVar {
				key: "LISTEN_FDS".into(),
				value: self.fds.len().to_string().into(),
			},
			EnvVar {
				key: "LISTEN_FDS_FIRST_FD".into(),
				value: self.fds.first().unwrap().as_raw_fd().to_string().into(),
			},
		]
	}
}

impl SocketSpec {
	fn create(&self) -> Result<OwnedFd> {
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

		let sock = socket(fam, ty, SockFlag::empty(), None).into_diagnostic()?;

		setsockopt(&sock, sockopt::ReuseAddr, &true).into_diagnostic()?;

		if matches!(fam, AddressFamily::Inet | AddressFamily::Inet6) {
			setsockopt(&sock, sockopt::ReusePort, &true).into_diagnostic()?;
		}

		bind(sock.as_raw_fd(), &addr).into_diagnostic()?;

		if let SocketType::Tcp = self.socket {
			listen(&sock, Backlog::new(1).unwrap()).into_diagnostic()?;
		}

		Ok(sock)
	}
}
