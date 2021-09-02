//! Synthetic event type, derived from inputs, triggers actions.
//!
//! Fundamentally, events in watchexec have three purposes:
//!
//! 1. To trigger the launch, restart, or other interruption of a process;
//! 2. To be filtered upon according to whatever set of criteria is desired;
//! 3. To carry information about what caused the event, which may be provided to the process.

use std::{
	collections::HashMap,
	fmt,
	path::{Path, PathBuf},
	process::ExitStatus,
};

use crate::signal::Signal;

/// An event, as far as watchexec cares about.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Event {
	pub particulars: Vec<Particle>,
	pub metadata: HashMap<String, Vec<String>>,
}

// TODO: this really needs a better name (along with "particulars")
/// Something which can be used to filter or qualify an event.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Particle {
	Path(PathBuf),
	Source(Source),
	Process(u32),
	Signal(Signal),
	ProcessCompletion(Option<ExitStatus>),
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
	Internal,
}

impl Event {
	/// Return all paths in the event's particulars.
	pub fn paths(&self) -> impl Iterator<Item = &Path> {
		self.particulars.iter().filter_map(|p| match p {
			Particle::Path(p) => Some(p.as_path()),
			_ => None,
		})
	}
	/// Return all signals in the event's particulars.
	pub fn signals(&self) -> impl Iterator<Item = Signal> + '_ {
		self.particulars.iter().filter_map(|p| match p {
			Particle::Signal(s) => Some(*s),
			_ => None,
		})
	}
}

impl fmt::Display for Event {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "Event")?;
		for p in &self.particulars {
			match p {
				Particle::Path(p) => write!(f, " path={}", p.display())?,
				Particle::Source(s) => write!(f, " source={:?}", s)?,
				Particle::Process(p) => write!(f, " process={}", p)?,
				Particle::Signal(s) => write!(f, " signal={:?}", s)?,
				Particle::ProcessCompletion(None) => write!(f, " command-completed")?,
				Particle::ProcessCompletion(Some(c)) => write!(f, " command-completed({})", c)?,
			}
		}

		if !self.metadata.is_empty() {
			write!(f, " meta: {:?}", self.metadata)?;
		}

		Ok(())
	}
}
