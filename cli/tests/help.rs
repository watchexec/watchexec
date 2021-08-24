use std::process::Command;

use assert_cmd::prelude::*;
use insta::assert_snapshot;

#[test]
fn help() {
	let output = Command::cargo_bin("watchexec")
		.unwrap()
		.arg("--help")
		.output()
		.unwrap();

	assert!(output.status.success(), "--help returns 0");
	assert_eq!(output.stderr, Vec::<u8>::new(), "--help stderr is empty");
	assert_snapshot!(
		if cfg!(windows) {
			"help_windows"
		} else {
			"help_unix"
		},
		String::from_utf8(output.stdout).unwrap()
	);
}
