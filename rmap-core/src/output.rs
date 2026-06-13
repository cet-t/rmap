//! Output model: compiled at load time to keep hot path fast (NFR-1).

use crate::{KeyCode, Modifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputToken {
    /// Synthesized key + optional mods
    Key { code: KeyCode, mods: Modifiers },
    /// Raw Unicode text (for IME feed or direct chars not producible by keystrokes)
    Text(String),
    /// Named special: {BS}, {Enter}, arrows, etc.
    Named(SpecialKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialKey {
    Backspace,
    Enter,
    Tab,
    Escape,
    Left,
    Right,
    Up,
    Down,
    // Add more as DvorakJ corpus requires: Home, End, PgUp, etc.
}

pub type OutputSeq = Vec<OutputToken>;

/// input_mode per layout (plan.md Output Model)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Direct,  // ASCII/Dvorak/Colemak etc. 1:1 key+mod
    Romaji,  // kana strings -> romaji keystrokes via bundled encoder
    Kana,    // kana strings -> JIS-kana key positions
}
