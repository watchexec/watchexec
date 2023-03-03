fn main() {
	embed_resource::compile("watchexec-manifest.rc");
	#[cfg(target_os = "linux")]
	shadow_rs::new().unwrap();
}
