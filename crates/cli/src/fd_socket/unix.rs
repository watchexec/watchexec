use std::{
	os::fd::{AsRawFd, OwnedFd},
	process,
};

use miette::{IntoDiagnostic, Result};

use super::{FdSpec, SocketType, Sockets};

#[derive(Debug)]
pub struct FdSockets {
	fds: Vec<OwnedFd>,
}

impl Sockets for FdSockets {
	async fn create(specs: &[FdSpec]) -> Result<Self> {
		debug_assert!(!specs.is_empty());
		specs
			.into_iter()
			.map(FdSpec::create)
			.collect::<Result<Vec<_>>>()
			.map(|fds| Self { fds })
	}

	fn envs(&self) -> Vec<(&'static str, String)> {
		vec![
			("LISTEN_FDS", self.fds.len().to_string()),
			(
				"LISTEN_FDS_FIRST_FD",
				self.fds.first().unwrap().as_raw_fd().to_string(),
			),
			("LISTEN_PID", process::id().to_string()),
		]
	}
}

impl FdSpec {
	fn create(&self) -> Result<OwnedFd> {
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

		let sock = socket(fam, ty, SockFlag::empty(), None).into_diagnostic()?;

		setsockopt(&sock, sockopt::ReuseAddr, &true).into_diagnostic()?;
		setsockopt(&sock, sockopt::ReusePort, &true).into_diagnostic()?;

		let rv = bind(sock.as_raw_fd(), &addr).and_then(|_| {
			if let SocketType::Tcp = self.socket {
				listen(&sock, Backlog::new(1).unwrap())?;
			}
			Ok(())
		});

		if rv.is_err() {
			unsafe { libc::close(sock.as_raw_fd()) };
		}

		rv.map(|_| sock).into_diagnostic()
	}
}
