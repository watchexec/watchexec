use std::{
	env::{args, var},
	io::ErrorKind,
};

use listenfd::ListenFd;

fn main() {
	eprintln!("LISTEN_FDS={:?}", var("LISTEN_FDS"));
	eprintln!("LISTEN_FDS_FIRST_FD={:?}", var("LISTEN_FDS_FIRST_FD"));
	eprintln!("LISTEN_PID={:?}", var("LISTEN_PID"));
	eprintln!("SYSTEMFD_SOCKET_SERVER={:?}", var("SYSTEMFD_SOCKET_SERVER"));
	eprintln!("SYSTEMFD_SOCKET_SECRET={:?}", var("SYSTEMFD_SOCKET_SECRET"));

	let mut listenfd = ListenFd::from_env();
	println!("\n{} sockets available\n", listenfd.len());

	for (n, arg) in args().skip(1).enumerate() {
		match arg.as_str() {
			"tcp" => {
				if let Ok(addr) = listenfd
					.take_tcp_listener(n)
					.and_then(|l| l.ok_or_else(|| ErrorKind::NotFound.into()))
					.expect(&format!("expected TCP listener at FD#{n}"))
					.local_addr()
				{
					println!("obtained TCP listener at FD#{n}, at addr {addr:?}");
				} else {
					println!("obtained TCP listener at FD#{n}, unknown addr");
				}
			}
			"udp" => {
				if let Ok(addr) = listenfd
					.take_udp_socket(n)
					.and_then(|l| l.ok_or_else(|| ErrorKind::NotFound.into()))
					.expect(&format!("expected UDP socket at FD#{n}"))
					.local_addr()
				{
					println!("obtained UDP socket at FD#{n}, at addr {addr:?}");
				} else {
					println!("obtained UDP socket at FD#{n}, unknown addr");
				}
			}
			#[cfg(unix)]
			"unix-stream" => {
				if let Ok(addr) = listenfd
					.take_unix_listener(n)
					.and_then(|l| l.ok_or_else(|| ErrorKind::NotFound.into()))
					.expect(&format!("expected Unix stream listener at FD#{n}"))
					.local_addr()
				{
					println!("obtained Unix stream listener at FD#{n}, at addr {addr:?}");
				} else {
					println!("obtained Unix stream listener at FD#{n}, unknown addr");
				}
			}
			#[cfg(unix)]
			"unix-datagram" => {
				if let Ok(addr) = listenfd
					.take_unix_datagram(n)
					.and_then(|l| l.ok_or_else(|| ErrorKind::NotFound.into()))
					.expect(&format!("expected Unix datagram socket at FD#{n}"))
					.local_addr()
				{
					println!("obtained Unix datagram socket at FD#{n}, at addr {addr:?}");
				} else {
					println!("obtained Unix datagram socket at FD#{n}, unknown addr");
				}
			}
			#[cfg(unix)]
			"unix-raw" => {
				let raw = listenfd
					.take_raw_fd(n)
					.and_then(|l| l.ok_or_else(|| ErrorKind::NotFound.into()))
					.expect(&format!("expected Unix raw socket at FD#{n}"));
				println!("obtained Unix raw socket at FD#{n}: {raw}");
			}
			other => {
				if cfg!(unix) {
					panic!("expected one of (tcp, udp, unix-stream, unix-datagram, unix-raw), found {other}")
				} else {
					panic!("expected one of (tcp, udp), found {other}")
				}
			}
		}
	}
}
