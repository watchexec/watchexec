include!(env!("BOSION_PATH"));

mod common;
fn main() {}

test_snapshot!(crate_version, Bosion::CRATE_VERSION);

test_snapshot!(crate_features, format!("{:#?}", Bosion::CRATE_FEATURES));

test_snapshot!(build_date, Bosion::BUILD_DATE);

test_snapshot!(build_datetime, Bosion::BUILD_DATETIME);

test_snapshot!(git_commit_hash, Bosion::GIT_COMMIT_HASH);

test_snapshot!(git_commit_shorthash, Bosion::GIT_COMMIT_SHORTHASH);

test_snapshot!(git_commit_date, Bosion::GIT_COMMIT_DATE);

test_snapshot!(git_commit_datetime, Bosion::GIT_COMMIT_DATETIME);

test_snapshot!(default_long_version, Bosion::LONG_VERSION);

test_snapshot!(
	default_long_version_with,
	Bosion::long_version_with(&[("extra", "field"), ("custom", "1.2.3")])
);
