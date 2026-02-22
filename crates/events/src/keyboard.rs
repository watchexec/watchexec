#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
/// A keyboard input.
pub enum Keyboard {
	/// Event representing an 'end of file' on stdin
	Eof,

	/// A key press in interactive mode
	Key {
		/// The key that was pressed.
		key: KeyCode,

		/// Modifier keys held during the press.
		#[cfg_attr(
			feature = "serde",
			serde(default, skip_serializing_if = "Modifiers::is_empty")
		)]
		modifiers: Modifiers,
	},
}

/// A key code.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
pub enum KeyCode {
	/// A unicode character (letter, digit, symbol, space).
	Char(char),
	/// Enter / Return.
	Enter,
	/// Escape.
	Escape,
}

/// Modifier key flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Modifiers {
	/// Ctrl / Control was held.
	#[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
	pub ctrl: bool,
	/// Alt / Option was held.
	#[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
	pub alt: bool,
	/// Shift was held.
	#[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
	pub shift: bool,
}

#[cfg(feature = "serde")]
fn is_false(b: &bool) -> bool {
	!b
}

impl Modifiers {
	/// Returns true if no modifier keys are set.
	#[must_use]
	pub fn is_empty(&self) -> bool {
		!self.ctrl && !self.alt && !self.shift
	}
}
