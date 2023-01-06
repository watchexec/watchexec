use std::env::args;

use miette::{IntoDiagnostic, Result};
use project_origins::origins;

// Run with: `cargo run --example find-origins [PATH]`
#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt::init();

	let first_arg = args().nth(1).unwrap_or_else(|| ".".to_string());
	let path = tokio::fs::canonicalize(first_arg).await.into_diagnostic()?;

	for origin in origins(&path).await {
		println!("{}", origin.display());
	}

	Ok(())
}
