#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> miette::Result<()> {
	watchexec_cli::run().await?;

	if std::process::id() == 1 {
		std::process::exit(0);
	}

	Ok(())
}
