use miette::IntoDiagnostic;

#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> miette::Result<()> {
	#[cfg(feature = "pid1")]
	pid1::Pid1Settings::new()
		.enable_log(cfg!(feature = "pid1-withlog"))
		.launch()
		.into_diagnostic()?;

	tokio::runtime::Builder::new_multi_thread()
		.enable_all()
		.build()
		.unwrap()
		.block_on(async { watchexec_cli::run().await })
}
