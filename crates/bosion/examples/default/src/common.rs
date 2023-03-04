#[cfg(test)]
pub(crate) fn git_commit_info(format: &str) -> String {
	let output = std::process::Command::new("git")
		.arg("show")
		.arg("--no-notes")
		.arg("--no-patch")
		.arg(format!("--pretty=format:{format}"))
		.output()
		.expect("git");

	String::from_utf8(output.stdout)
		.expect("git")
		.trim()
		.to_string()
}

#[macro_export]
macro_rules! test_snapshot {
	($name:ident, $actual:expr) => {
		#[cfg(test)]
		#[test]
		fn $name() {
			::snapbox::Assert::new().matches(
				::leon::Template::parse(
					std::fs::read_to_string(format!("../snapshots/{}.txt", stringify!($name)))
						.expect("read file")
						.trim(),
				)
				.expect("leon parse")
				.render(&[
					(
						"today date".to_string(),
						::time::OffsetDateTime::now_utc()
							.format(::time::macros::format_description!("[year]-[month]-[day]"))
							.unwrap(),
					),
					("git hash".to_string(), crate::common::git_commit_info("%H")),
					("git shorthash".to_string(), crate::common::git_commit_info("%h")),
					("git date".to_string(), crate::common::git_commit_info("%cs")),
					(
						"git datetime".to_string(),
						crate::common::git_commit_info("%ci")
							.split_whitespace()
							.take(2)
							.collect::<Vec<_>>()
							.join(" "),
					),
				])
				.expect("leon render"),
				$actual,
			);
		}
	};
}
