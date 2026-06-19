//! DvorakJ layout file parser.
//!
//! Parses DvorakJ-style `.txt` layout files (Shift-JIS / UTF-8) into the
//! [`rmap_core::layout::Layout`] structure.  This crate is intentionally
//! separated from `rmap-core` so that the parser can be reused or replaced
//! independently of the core engine.

mod block;
mod cell;
mod grid;
mod keymap;
mod parse;

use cell::KanaEncoder;
use rmap_core::{
    layout::Layout,
    loader::{LayoutLoader, LoadError},
    KeyboardLayout,
};

pub struct DvorakJLayoutLoader {
    kana_encoder: KanaEncoder,
}

impl DvorakJLayoutLoader {
    pub fn new() -> Self {
        Self {
            kana_encoder: KanaEncoder,
        }
    }
}

impl Default for DvorakJLayoutLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutLoader for DvorakJLayoutLoader {
    fn format_name(&self) -> &'static str {
        "dvorakj"
    }

    fn load(&self, bytes: &[u8], id: &str) -> Result<Layout, LoadError> {
        let (keyboard, text) = if id.ends_with(".en.txt") {
            (
                KeyboardLayout::Us,
                String::from_utf8_lossy(bytes).into_owned(),
            )
        } else if id.ends_with(".jp.txt") {
            (
                KeyboardLayout::Jis,
                encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned(),
            )
        } else {
            let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
            let text = match std::str::from_utf8(bytes) {
                Ok(s) => s.to_string(),
                Err(_) => encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned(),
            };
            (KeyboardLayout::Jis, text)
        };
        let stripped = strip_comments(&text);
        parse::parse_dvorakj(&stripped, id, &self.kana_encoder, keyboard)
    }
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if c == '/' && it.peek() == Some(&'*') {
            it.next();
            while let Some(c2) = it.next() {
                if c2 == '*' && it.peek() == Some(&'/') {
                    it.next();
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
