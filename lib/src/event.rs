//! Fundamentally, events in watchexec have three purposes:
//!
//! 1. To trigger the launch, restart, or other interruption of a process;
//! 2. To be filtered upon according to whatever set of criteria is desired;
//! 3. To carry information about what caused the event, which may be provided to the process.

use chrono::{DateTime, Local};
use std::{collections::HashMap, path::PathBuf};

use crate::signal::Signal;

/// An event, as far as watchexec cares about.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Event {
	pub particulars: Vec<Particle>,
	pub metadata: HashMap<String, Vec<String>>,
}

/// Something which can be used to filter an event.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Particle {
	Time(DateTime<Local>),
	Path(PathBuf),
	Source(Source),
	Process(u32),
	Signal(Signal),
}

/// The general origin of the event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Source {
	Filesystem,
	Keyboard,
	Mouse,
	Os,
	Time,
}
