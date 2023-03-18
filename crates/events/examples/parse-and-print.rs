use std::io::{stdin, Result};
use watchexec_events::Event;

fn main() -> Result<()> {
	for line in stdin().lines() {
		let event: Event = serde_json::from_str(&line?)?;
		dbg!(event);
	}

	Ok(())
}
