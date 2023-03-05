include!(env!("BOSION_PATH"));

#[path = "../../default/src/common.rs"]
mod common;
fn main() {}

test_snapshot!(crate_version, Bosion::CRATE_VERSION);

test_snapshot!(crate_features, format!("{:#?}", Bosion::CRATE_FEATURES));

test_snapshot!(build_date, Bosion::BUILD_DATE);

test_snapshot!(build_datetime, Bosion::BUILD_DATETIME);

test_snapshot!(no_git_long_version, Bosion::LONG_VERSION);

test_snapshot!(
	no_git_long_version_with,
	Bosion::long_version_with(&[("extra", "field"), ("custom", "1.2.3")])
);
