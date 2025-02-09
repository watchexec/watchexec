use crate::args::Args;

use super::*;
use clap::{builder::TypedValueParser, CommandFactory};
use std::{
	ffi::OsStr,
	net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
};

#[test]
fn parse_port_only() {
	let cmd = Args::command();
	assert_eq!(
		SocketSpecValueParser
			.parse_ref(&cmd, None, OsStr::new("8080"))
			.unwrap(),
		SocketSpec {
			socket: SocketType::Tcp,
			addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080)),
		}
	);
}

#[test]
fn parse_addr_port_v4() {
	let cmd = Args::command();
	assert_eq!(
		SocketSpecValueParser
			.parse_ref(&cmd, None, OsStr::new("1.2.3.4:38192"))
			.unwrap(),
		SocketSpec {
			socket: SocketType::Tcp,
			addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 38192)),
		}
	);
}

#[test]
fn parse_addr_port_v6() {
	let cmd = Args::command();
	assert_eq!(
		SocketSpecValueParser
			.parse_ref(&cmd, None, OsStr::new("[ff64::1234]:81"))
			.unwrap(),
		SocketSpec {
			socket: SocketType::Tcp,
			addr: SocketAddr::V6(SocketAddrV6::new(
				Ipv6Addr::new(0xff64, 0, 0, 0, 0, 0, 0, 0x1234),
				81,
				0,
				0
			)),
		}
	);
}

#[test]
fn parse_port_only_explicit_tcp() {
	let cmd = Args::command();
	assert_eq!(
		SocketSpecValueParser
			.parse_ref(&cmd, None, OsStr::new("tcp::443"))
			.unwrap(),
		SocketSpec {
			socket: SocketType::Tcp,
			addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 443)),
		}
	);
}

#[test]
fn parse_addr_port_v4_explicit_tcp() {
	let cmd = Args::command();
	assert_eq!(
		SocketSpecValueParser
			.parse_ref(&cmd, None, OsStr::new("tcp::1.2.3.4:38192"))
			.unwrap(),
		SocketSpec {
			socket: SocketType::Tcp,
			addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 38192)),
		}
	);
}

#[test]
fn parse_addr_port_v6_explicit_tcp() {
	let cmd = Args::command();
	assert_eq!(
		SocketSpecValueParser
			.parse_ref(&cmd, None, OsStr::new("tcp::[ff64::1234]:81"))
			.unwrap(),
		SocketSpec {
			socket: SocketType::Tcp,
			addr: SocketAddr::V6(SocketAddrV6::new(
				Ipv6Addr::new(0xff64, 0, 0, 0, 0, 0, 0, 0x1234),
				81,
				0,
				0
			)),
		}
	);
}

#[test]
fn parse_port_only_explicit_udp() {
	let cmd = Args::command();
	assert_eq!(
		SocketSpecValueParser
			.parse_ref(&cmd, None, OsStr::new("udp::443"))
			.unwrap(),
		SocketSpec {
			socket: SocketType::Udp,
			addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 443)),
		}
	);
}

#[test]
fn parse_addr_port_v4_explicit_udp() {
	let cmd = Args::command();
	assert_eq!(
		SocketSpecValueParser
			.parse_ref(&cmd, None, OsStr::new("udp::1.2.3.4:38192"))
			.unwrap(),
		SocketSpec {
			socket: SocketType::Udp,
			addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(1, 2, 3, 4), 38192)),
		}
	);
}

#[test]
fn parse_addr_port_v6_explicit_udp() {
	let cmd = Args::command();
	assert_eq!(
		SocketSpecValueParser
			.parse_ref(&cmd, None, OsStr::new("udp::[ff64::1234]:81"))
			.unwrap(),
		SocketSpec {
			socket: SocketType::Udp,
			addr: SocketAddr::V6(SocketAddrV6::new(
				Ipv6Addr::new(0xff64, 0, 0, 0, 0, 0, 0, 0x1234),
				81,
				0,
				0
			)),
		}
	);
}

#[test]
fn parse_bad_prefix() {
	let cmd = Args::command();
	assert_eq!(
		SocketSpecValueParser
			.parse_ref(&cmd, None, OsStr::new("gopher::777"))
			.unwrap_err()
			.to_string(),
		String::from(r#"error: invalid prefix "gopher""#),
	);
}

#[test]
fn parse_bad_port_zero() {
	let cmd = Args::command();
	assert_eq!(
		SocketSpecValueParser
			.parse_ref(&cmd, None, OsStr::new("0"))
			.unwrap_err()
			.to_string(),
		String::from("error: invalid port number: cannot be zero"),
	);
}

#[test]
fn parse_bad_port_high() {
	let cmd = Args::command();
	assert_eq!(
		SocketSpecValueParser
			.parse_ref(&cmd, None, OsStr::new("100000"))
			.unwrap_err()
			.to_string(),
		String::from("error: invalid port number: greater than 65535"),
	);
}

#[test]
fn parse_bad_port_alpha() {
	let cmd = Args::command();
	assert_eq!(
		SocketSpecValueParser
			.parse_ref(&cmd, None, OsStr::new("port"))
			.unwrap_err()
			.to_string(),
		String::from("error: invalid port number"),
	);
}
