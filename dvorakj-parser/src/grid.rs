//! Grid parsing: `|`-delimited rows → physical-key output map.

use crate::cell::{compile_cell, KanaEncoder};
use rmap_core::{loader::LoadError, InputMode, KeyCode, KeyboardLayout, OutputSeq};
use std::collections::HashMap;

pub(crate) fn parse_grid(
    body: &[String],
    encoder: &KanaEncoder,
    mode: InputMode,
    row_offset: usize,
    keyboard: KeyboardLayout,
) -> Result<HashMap<KeyCode, OutputSeq>, LoadError> {
    let mut out = HashMap::new();
    for (r, line) in body.iter().enumerate() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        let phys = physical_row(r + row_offset, keyboard);
        if phys.is_empty() {
            continue;
        }
        let n = std::cmp::min(cells.len(), phys.len());
        for i in 0..n {
            let cell = cells[i];
            if cell.is_empty() || cell == "@@@" {
                continue;
            }
            let seq = compile_cell(cell, mode, encoder, keyboard)?;
            if !seq.is_empty() {
                out.insert(phys[i], seq);
            }
        }
    }
    Ok(out)
}

fn physical_row(row: usize, keyboard: KeyboardLayout) -> &'static [KeyCode] {
    match (keyboard, row) {
        (KeyboardLayout::Jis, 0) => &[
            KeyCode::Num1,
            KeyCode::Num2,
            KeyCode::Num3,
            KeyCode::Num4,
            KeyCode::Num5,
            KeyCode::Num6,
            KeyCode::Num7,
            KeyCode::Num8,
            KeyCode::Num9,
            KeyCode::Num0,
            KeyCode::Minus,
            KeyCode::Caret,
            KeyCode::Yen,
        ],
        (KeyboardLayout::Jis, 1) => &[
            KeyCode::Q,
            KeyCode::W,
            KeyCode::E,
            KeyCode::R,
            KeyCode::T,
            KeyCode::Y,
            KeyCode::U,
            KeyCode::I,
            KeyCode::O,
            KeyCode::P,
            KeyCode::AtSign,
            KeyCode::LBracket,
        ],
        (KeyboardLayout::Jis, 2) => &[
            KeyCode::A,
            KeyCode::S,
            KeyCode::D,
            KeyCode::F,
            KeyCode::G,
            KeyCode::H,
            KeyCode::J,
            KeyCode::K,
            KeyCode::L,
            KeyCode::Semicolon,
            KeyCode::Colon,
            KeyCode::RBracket,
        ],
        (KeyboardLayout::Jis, 3) => &[
            KeyCode::Z,
            KeyCode::X,
            KeyCode::C,
            KeyCode::V,
            KeyCode::B,
            KeyCode::N,
            KeyCode::M,
            KeyCode::Comma,
            KeyCode::Dot,
            KeyCode::Slash,
            KeyCode::Backslash,
        ],

        (KeyboardLayout::Us, 0) => &[
            KeyCode::Num1,
            KeyCode::Num2,
            KeyCode::Num3,
            KeyCode::Num4,
            KeyCode::Num5,
            KeyCode::Num6,
            KeyCode::Num7,
            KeyCode::Num8,
            KeyCode::Num9,
            KeyCode::Num0,
            KeyCode::Minus,
            KeyCode::Equal,
            KeyCode::Grave,
        ],
        (KeyboardLayout::Us, 1) => &[
            KeyCode::Q,
            KeyCode::W,
            KeyCode::E,
            KeyCode::R,
            KeyCode::T,
            KeyCode::Y,
            KeyCode::U,
            KeyCode::I,
            KeyCode::O,
            KeyCode::P,
            KeyCode::LBracket,
            KeyCode::RBracket,
            KeyCode::Backslash,
        ],
        (KeyboardLayout::Us, 2) => &[
            KeyCode::A,
            KeyCode::S,
            KeyCode::D,
            KeyCode::F,
            KeyCode::G,
            KeyCode::H,
            KeyCode::J,
            KeyCode::K,
            KeyCode::L,
            KeyCode::Semicolon,
            KeyCode::Quote,
        ],
        (KeyboardLayout::Us, 3) => &[
            KeyCode::Z,
            KeyCode::X,
            KeyCode::C,
            KeyCode::V,
            KeyCode::B,
            KeyCode::N,
            KeyCode::M,
            KeyCode::Comma,
            KeyCode::Dot,
            KeyCode::Slash,
        ],

        (_, _) => &[],
    }
}
