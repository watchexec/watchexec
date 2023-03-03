fn main() {
	embed_resource::compile("watchexec-manifest.rc");
	shadow_rs::new().unwrap();
}
