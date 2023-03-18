use std::{ffi::OsString, fmt::Write, path::PathBuf};

use miette::{IntoDiagnostic, Result};
use watchexec::paths::summarise_events_to_env;
use watchexec_events::{filekind::FileEventKind, Event, Tag};

use crate::state::RotatingTempFile;

pub fn emits_to_environment(events: &[Event]) -> impl Iterator<Item = (String, OsString)> {
	summarise_events_to_env(events.iter())
		.into_iter()
		.map(|(k, v)| (format!("WATCHEXEC_{k}_PATH"), v))
}

fn events_to_simple_format(events: &[Event]) -> Result<String> {
	let mut buf = String::new();
	for event in events {
		let feks = event
			.tags
			.iter()
			.filter_map(|tag| match tag {
				Tag::FileEventKind(kind) => Some(kind),
				_ => None,
			})
			.collect::<Vec<_>>();

		for path in event.paths().map(|(p, _)| p) {
			if feks.is_empty() {
				writeln!(&mut buf, "other:{}", path.to_string_lossy()).into_diagnostic()?;
				continue;
			}

			for fek in &feks {
				writeln!(
					&mut buf,
					"{}:{}",
					match fek {
						FileEventKind::Any | FileEventKind::Other => "other",
						FileEventKind::Access(_) => "access",
						FileEventKind::Create(_) => "create",
						FileEventKind::Modify(_) => "modify",
						FileEventKind::Remove(_) => "remove",
					},
					path.to_string_lossy()
				)
				.into_diagnostic()?;
			}
		}
	}

	Ok(buf)
}

pub fn emits_to_file(target: &RotatingTempFile, events: &[Event]) -> Result<PathBuf> {
	target.rotate()?;
	target.write(events_to_simple_format(events)?.as_bytes())?;
	Ok(target.path())
}

pub fn emits_to_json_file(target: &RotatingTempFile, events: &[Event]) -> Result<PathBuf> {
	target.rotate()?;
	for event in events {
		if event.is_empty() {
			continue;
		}

		target.write(&serde_json::to_vec(event).into_diagnostic()?)?;
		target.write(b"\n")?;
	}
	Ok(target.path())
}
