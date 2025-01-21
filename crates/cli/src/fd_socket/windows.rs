use std::{
	io::ErrorKind,
	net::SocketAddr,
	os::windows::io::{AsRawSocket, OwnedSocket},
	str::FromStr,
	sync::Arc,
};

use miette::{IntoDiagnostic, Result};
use tokio::{
	io::{AsyncReadExt, AsyncWriteExt},
	net::{TcpListener, TcpStream},
	task::spawn,
};
use tracing::instrument;
use uuid::Uuid;
use windows_sys::Win32::Networking::WinSock::{WSADuplicateSocketW, SOCKET, WSAPROTOCOL_INFOW};

use crate::args::command::EnvVar;

use super::{FdSpec, SocketType, Sockets};

#[derive(Debug)]
pub struct FdSockets {
	sockets: Arc<[OwnedSocket]>,
	secret: Uuid,
	server: Option<TcpListener>,
	server_addr: SocketAddr,
}

impl Sockets for FdSockets {
	#[instrument(level = "trace")]
	async fn create(specs: &[FdSpec]) -> Result<Self> {
		debug_assert!(!specs.is_empty());
		let sockets = specs
			.into_iter()
			.map(FdSpec::create)
			.collect::<Result<Vec<_>>>()?;

		let server = TcpListener::bind("127.0.0.1:0").await.into_diagnostic()?;
		let server_addr = server.local_addr().into_diagnostic()?;

		Ok(Self {
			sockets: sockets.into(),
			secret: Uuid::new_v4(),
			server: Some(server),
			server_addr,
		})
	}

	#[instrument(level = "trace")]
	fn envs(&self) -> impl Iterator<Item = EnvVar> {
		vec![
			EnvVar {
				key: "SYSTEMFD_SOCKET_SERVER".into(),
				value: self.server_addr.to_string().into(),
			},
			EnvVar {
				key: "SYSTEMFD_SOCKET_SECRET".into(),
				value: self.secret.to_string().into(),
			},
		]
		.into_iter()
	}

	#[instrument(level = "trace", skip(self))]
	fn serve(&mut self) {
		let listener = self.server.take().unwrap();
		let secret = self.secret;
		let sockets = self.sockets.clone();
		spawn(async move {
			loop {
				let Ok((stream, _)) = listener.accept().await else {
					break;
				};

				spawn(provide_sockets(stream, sockets.clone(), secret));
			}
		});
	}
}

async fn provide_sockets(
	mut stream: TcpStream,
	sockets: Arc<[OwnedSocket]>,
	secret: Uuid,
) -> std::io::Result<()> {
	let mut data = Vec::new();
	stream.read_to_end(&mut data).await?;
	let Ok(out) = String::from_utf8(data) else {
		return Err(ErrorKind::InvalidInput.into());
	};

	let Some((challenge, pid)) = out.split_once('|') else {
		return Err(ErrorKind::InvalidInput.into());
	};

	let Ok(uuid) = Uuid::from_str(challenge) else {
		return Err(ErrorKind::InvalidInput.into());
	};

	let Ok(pid) = u32::from_str(pid) else {
		return Err(ErrorKind::InvalidInput.into());
	};

	if uuid != secret {
		return Err(ErrorKind::InvalidData.into());
	}

	for socket in sockets.iter() {
		let payload = socket_to_payload(socket, pid)?;
		stream.write_all(&payload).await?;
	}

	stream.shutdown().await
}

fn socket_to_payload(socket: &OwnedSocket, pid: u32) -> std::io::Result<Vec<u8>> {
	// SAFETY:
	// - we're not reading from this until it gets populated by WSADuplicateSocketW
	// - the struct is entirely integers and arrays of integers
	let mut proto_info: WSAPROTOCOL_INFOW = unsafe { std::mem::zeroed() };

	// SAFETY: ffi
	if unsafe { WSADuplicateSocketW(socket.as_raw_socket() as SOCKET, pid, &mut proto_info) } != 0 {
		return Err(ErrorKind::InvalidData.into());
	}

	// SAFETY:
	// - non-nullability, alignment, and contiguousness are taken care of by serialising a single value
	// - WSAPROTOCOL_INFOW is repr(C)
	// - we don't mutate that memory (we immediately to_vec it)
	// - we have its exact size
	Ok(unsafe {
		let bytes: *const u8 = &proto_info as *const WSAPROTOCOL_INFOW as *const _;
		std::slice::from_raw_parts(bytes, std::mem::size_of::<WSAPROTOCOL_INFOW>())
	}
	.to_vec())
}

impl FdSpec {
	fn create(&self) -> Result<OwnedSocket> {
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
