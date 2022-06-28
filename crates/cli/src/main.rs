#[tokio::main]
async fn main() -> miette::Result<()> {
	watchexec_cli::run().await
}
