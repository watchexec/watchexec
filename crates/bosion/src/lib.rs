#![doc = include_str!("../README.md")]

use std::{env::var, fs::File, io::Write, path::PathBuf};

pub use info::*;
mod info;

/// Gather build-time information for the current crate
///
/// See the crate-level documentation for a guide. This function is a convenience wrapper around
/// [`gather_to`] with the most common defaults: it writes to `bosion.rs` a pub(crate) struct named
/// `Bosion`.
pub fn gather() {
	gather_to("bosion.rs", "Bosion", false);
}

/// Gather build-time information for the current crate (public visibility)
///
/// See the crate-level documentation for a guide. This function is a convenience wrapper around
/// [`gather_to`]: it writes to `bosion.rs` a pub struct named `Bosion`.
pub fn gather_pub() {
	gather_to("bosion.rs", "Bosion", true);
}

/// Gather build-time information for the current crate (custom output)
///
/// Gathers a limited set of build-time information for the current crate and writes it to a file.
/// The file is always written to the `OUT_DIR` directory, as per Cargo conventions. It contains a
/// zero-size struct with a bunch of associated constants containing the gathered information, and a
/// `long_version_with` function (when the `std` feature is enabled) that takes a slice of extra
/// key-value pairs to append in the same format.
///
/// `public` controls whether the struct is `pub` (true) or `pub(crate)` (false).
///
/// The generated code is entirely documented, and will appear in your documentation (in docs.rs, it
/// only will if visibility is public).
///
/// See [`Info`] for a list of gathered data.
///
/// The constants include all the information from [`Info`], as well as the following:
///
/// - `LONG_VERSION`: A clap-ready long version string, including the crate version, features, build
///   date, and git information when available.
/// - `CRATE_FEATURE_STRING`: A string containing the crate features, in the format `+feat1 +feat2`.
///
/// We also instruct rustc to rerun the build script if the environment changes, as necessary.
pub fn gather_to(filename: &str, structname: &str, public: bool) {
	let path = PathBuf::from(var("OUT_DIR").expect("bosion")).join(filename);
	println!("cargo:rustc-env=BOSION_PATH={}", path.display());

	let info = Info::gather().expect("bosion");
	info.set_reruns();
	let Info {
		crate_version,
		crate_features,
		build_date,
		build_datetime,
		git,
	} = info;

	let crate_feature_string = crate_features
		.iter()
		.filter(|feat| *feat != "default")
		.map(|feat| format!("+{feat}"))
		.collect::<Vec<_>>()
		.join(" ");

	let crate_feature_list = crate_features.join(",");

	let viz = if public { "pub" } else { "pub(crate)" };

	let (git_render, long_version) = if let Some(GitInfo {
		git_hash,
		git_shorthash,
		git_date,
		git_datetime,
		..
	}) = git
	{
		(format!(
		"
			/// The git commit hash
			///
			/// This is the full hash of the commit that was built. Note that if the repository was
			/// dirty, this will be the hash of the last commit, not including the changes.
			pub const GIT_COMMIT_HASH: &'static str = {git_hash:?};

			/// The git commit hash, shortened
			///
			/// This is the shortened hash of the commit that was built. Same caveats as with
			/// `GIT_COMMIT_HASH` apply. The length of the hash is as short as possible while still
			/// being unambiguous, at build time. For large repositories, this may be longer than 7
			/// characters.
			pub const GIT_COMMIT_SHORTHASH: &'static str = {git_shorthash:?};

			/// The git commit date
			///
			/// This is the date (`YYYY-MM-DD`) of the commit that was built. Same caveats as with
			/// `GIT_COMMIT_HASH` apply.
			pub const GIT_COMMIT_DATE: &'static str = {git_date:?};

			/// The git commit date and time
			///
			/// This is the date and time (`YYYY-MM-DD HH:MM:SS`) of the commit that was built. Same
			/// caveats as with `GIT_COMMIT_HASH` apply.
			pub const GIT_COMMIT_DATETIME: &'static str = {git_datetime:?};
		"
	), format!("{crate_version} ({git_shorthash} {git_date}) {crate_feature_string}\ncommit-hash: {git_hash}\ncommit-date: {git_date}\nbuild-date: {build_date}\nrelease: {crate_version}\nfeatures: {crate_feature_list}"))
	} else {
		("".to_string(), format!("{crate_version} ({build_date}) {crate_feature_string}\nbuild-date: {build_date}\nrelease: {crate_version}\nfeatures: {crate_feature_list}"))
	};

	#[cfg(all(feature = "std"))]
	let long_version_with_fn = r#"
		/// Returns the long version string with extra information tacked on
		///
		/// This is the same as `LONG_VERSION` but takes a slice of key-value pairs to append to the
		/// end in the same format.
		pub fn long_version_with(extra: &[(&str, &str)]) -> String {
			let mut output = Self::LONG_VERSION.to_string();

			for (k, v) in extra {
				output.push_str(&format!("\n{}: {}", k, v));
			}

			output
		}
	"#;
	#[cfg(not(feature = "std"))]
	let long_version_with_fn = "";

	let bosion_version = env!("CARGO_PKG_VERSION");
	let render = format!(
		r#"
		/// Build-time information
		///
		/// This struct is generated by the [bosion](https://docs.rs/bosion) crate at build time.
		///
		/// Bosion version: {bosion_version}
		#[derive(Debug, Clone, Copy)]
		{viz} struct {structname};

		#[allow(dead_code)]
		impl {structname} {{
			/// Clap-compatible long version string
			///
			/// At minimum, this will be the crate version and build date.
			///
			/// It presents as a first "summary" line like `crate_version (build_date) features`,
			/// followed by `key: value` pairs. This is the same format used by `rustc -Vv`.
			///
			/// If git info is available, it also includes the git hash, short hash and commit date,
			/// and swaps the build date for the commit date in the summary line.
			pub const LONG_VERSION: &'static str = {long_version:?};

			/// The crate version, as reported by Cargo
			///
			/// You should probably prefer reading the `CARGO_PKG_VERSION` environment variable.
			pub const CRATE_VERSION: &'static str = {crate_version:?};

			/// The crate features
			///
			/// This is a list of the features that were enabled when this crate was built,
			/// lowercased and with underscores replaced by hyphens.
			pub const CRATE_FEATURES: &'static [&'static str] = &{crate_features:?};

			/// The crate features, as a string
			///
			/// This is in format `+feature +feature2 +feature3`, lowercased with underscores
			/// replaced by hyphens.
			pub const CRATE_FEATURE_STRING: &'static str = {crate_feature_string:?};

			/// The build date
			///
			/// This is the date that the crate was built, in the format `YYYY-MM-DD`. If the
			/// environment variable `SOURCE_DATE_EPOCH` was set, it's used instead of the current
			/// time, for [reproducible builds](https://reproducible-builds.org/).
			pub const BUILD_DATE: &'static str = {build_date:?};

			/// The build datetime
			///
			/// This is the date and time that the crate was built, in the format
			/// `YYYY-MM-DD HH:MM:SS`. If the environment variable `SOURCE_DATE_EPOCH` was set, it's
			/// used instead of the current time, for
			/// [reproducible builds](https://reproducible-builds.org/).
			pub const BUILD_DATETIME: &'static str = {build_datetime:?};

			{git_render}

			{long_version_with_fn}
		}}
		"#
	);

	let mut file = File::create(path).expect("bosion");
	file.write_all(render.as_bytes()).expect("bosion");
}

/// Gather build-time information and write it to the environment
///
/// See the crate-level documentation for a guide. This function is a convenience wrapper around
/// [`gather_to_env_with_prefix`] with the most common default prefix of `BOSION_`.
pub fn gather_to_env() {
	gather_to_env_with_prefix("BOSION_");
}

/// Gather build-time information and write it to the environment
///
/// Gathers a limited set of build-time information for the current crate and makes it available to
/// the crate as build environment variables. This is an alternative to [`include!`]ing a file which
/// is generated at build time, like for [`gather`] and variants, which doesn't create any new code
/// and doesn't include any information in the binary that you do not explicitly use.
///
/// The environment variables are prefixed with the given string, which should be generally be
/// uppercase and end with an underscore.
///
/// See [`Info`] for a list of gathered data.
///
/// Unlike [`gather`], there is no Clap-ready `LONG_VERSION` string, but you can of course generate
/// one yourself from the environment variables.
///
/// We also instruct rustc to rerun the build script if the environment changes, as necessary.
pub fn gather_to_env_with_prefix(prefix: &str) {
	let info = Info::gather().expect("bosion");
	info.set_reruns();
	let Info {
		crate_version,
		crate_features,
		build_date,
		build_datetime,
		git,
	} = info;

	println!("cargo:rustc-env={prefix}CRATE_VERSION={crate_version}");
	println!(
		"cargo:rustc-env={prefix}CRATE_FEATURES={}",
		crate_features.join(",")
	);
	println!("cargo:rustc-env={prefix}BUILD_DATE={build_date}");
	println!("cargo:rustc-env={prefix}BUILD_DATETIME={build_datetime}");

	if let Some(GitInfo {
		git_hash,
		git_shorthash,
		git_date,
		git_datetime,
		..
	}) = git
	{
		println!("cargo:rustc-env={prefix}GIT_COMMIT_HASH={git_hash}");
		println!("cargo:rustc-env={prefix}GIT_COMMIT_SHORTHASH={git_shorthash}");
		println!("cargo:rustc-env={prefix}GIT_COMMIT_DATE={git_date}");
		println!("cargo:rustc-env={prefix}GIT_COMMIT_DATETIME={git_datetime}");
	}
}
