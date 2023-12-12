fn main() {
	embed_resource::compile("watchexec-manifest.rc", embed_resource::NONE);
	bosion::gather();

	if std::env::var("CARGO_FEATURE_EYRA").is_ok() {
		println!("cargo:rustc-link-arg=-nostartfiles");
	}
}
