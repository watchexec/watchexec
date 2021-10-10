use std::env::args;

use miette::{IntoDiagnostic, Result};
use watchexec::project::origins;

// Run with: `cargo run --example project-origins [PATH]`
#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt::init();

	let path =
		dunce::canonicalize(args().nth(1).unwrap_or_else(|| ".".to_string())).into_diagnostic()?;
	for origin in origins(&path).await {
		println!("{}", origin.display());
	}

	Ok(())
}
