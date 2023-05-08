fn main() {
	embed_resource::compile("watchexec-manifest.rc", embed_resource::NONE);
	bosion::gather();
}
