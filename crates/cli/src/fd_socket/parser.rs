use std::{
	ffi::OsStr,
	net::{IpAddr, Ipv4Addr, SocketAddr},
	num::{IntErrorKind, NonZero},
	str::FromStr,
};

use clap::{
	builder::TypedValueParser,
	error::{Error, ErrorKind},
};
use miette::Result;

use super::{FdSpec, SocketType};

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
