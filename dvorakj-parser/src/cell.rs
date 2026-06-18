//! Cell compilation: text/token → OutputToken sequence.

use rmap_core::{
    loader::LoadError, InputMode, KeyCode, KeyboardLayout, Modifiers, OutputSeq, OutputToken,
    SpecialKey,
};

pub(crate) fn compile_cell(
    cell: &str,
    mode: InputMode,
    encoder: &KanaEncoder,
    keyboard: KeyboardLayout,
) -> Result<OutputSeq, LoadError> {
    let c = cell.trim();
    if c.is_empty() || c == "@@@" {
        return Ok(vec![]);
    }
    if mode == InputMode::Romaji && !c.contains('{') {
        return Ok(encoder.encode(c, mode));
    }
    let mut seq = vec![];
    let mut chars = c.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut inner = String::new();
            let mut closed = false;
            for nc in chars.by_ref() {
                if nc == '}' {
                    closed = true;
                    break;
                }
                inner.push(nc);
            }
            if closed {
                seq.push(brace_token(&inner, keyboard));
            } else {
                seq.push(key_or_text('{', keyboard));
                for ic in inner.chars() {
                    seq.push(key_or_text(ic, keyboard));
                }
            }
        } else {
            seq.push(key_or_text(ch, keyboard));
        }
    }
    Ok(seq)
}

fn brace_token(inner: &str, keyboard: KeyboardLayout) -> OutputToken {
    let s = inner.to_lowercase();
    match s.as_str() {
        "bs" | "backspace" => OutputToken::Named(SpecialKey::Backspace),
        "enter" | "return" => OutputToken::Named(SpecialKey::Enter),
        "tab" => OutputToken::Named(SpecialKey::Tab),
        "esc" | "escape" => OutputToken::Named(SpecialKey::Escape),
        "left" => OutputToken::Named(SpecialKey::Left),
        "right" => OutputToken::Named(SpecialKey::Right),
        "up" => OutputToken::Named(SpecialKey::Up),
        "down" => OutputToken::Named(SpecialKey::Down),
        "space" => OutputToken::Key {
            code: KeyCode::Space,
            mods: Modifiers::empty(),
        },
        "pipe" | "bar" => OutputToken::Text("|".to_string()),
        _ if s.len() == 1 => key_or_text(s.chars().next().unwrap(), keyboard),
        _ => OutputToken::Text(format!("{{{}}}", inner)),
    }
}

pub(crate) fn key_or_text(ch: char, keyboard: KeyboardLayout) -> OutputToken {
    if keyboard == KeyboardLayout::Us && !ch.is_ascii_alphanumeric() {
        return OutputToken::Text(ch.to_string());
    }
    let code = ascii_to_keycode(ch);
    if matches!(code, KeyCode::Unknown(_)) {
        OutputToken::Text(ch.to_string())
    } else {
        let mods = if ch.is_ascii_uppercase() {
            Modifiers::SHIFT
        } else {
            Modifiers::empty()
        };
        OutputToken::Key { code, mods }
    }
}

pub(crate) fn ascii_to_keycode(c: char) -> KeyCode {
    match c.to_ascii_lowercase() {
        'a' => KeyCode::A,
        'b' => KeyCode::B,
        'c' => KeyCode::C,
        'd' => KeyCode::D,
        'e' => KeyCode::E,
        'f' => KeyCode::F,
        'g' => KeyCode::G,
        'h' => KeyCode::H,
        'i' => KeyCode::I,
        'j' => KeyCode::J,
        'k' => KeyCode::K,
        'l' => KeyCode::L,
        'm' => KeyCode::M,
        'n' => KeyCode::N,
        'o' => KeyCode::O,
        'p' => KeyCode::P,
        'q' => KeyCode::Q,
        'r' => KeyCode::R,
        's' => KeyCode::S,
        't' => KeyCode::T,
        'u' => KeyCode::U,
        'v' => KeyCode::V,
        'w' => KeyCode::W,
        'x' => KeyCode::X,
        'y' => KeyCode::Y,
        'z' => KeyCode::Z,
        '0' => KeyCode::Num0,
        '1' => KeyCode::Num1,
        '2' => KeyCode::Num2,
        '3' => KeyCode::Num3,
        '4' => KeyCode::Num4,
        '5' => KeyCode::Num5,
        '6' => KeyCode::Num6,
        '7' => KeyCode::Num7,
        '8' => KeyCode::Num8,
        '9' => KeyCode::Num9,
        '-' => KeyCode::Minus,
        '=' => KeyCode::Equal,
        '[' => KeyCode::LBracket,
        ']' => KeyCode::RBracket,
        '\\' => KeyCode::Backslash,
        ';' => KeyCode::Semicolon,
        '\'' => KeyCode::Quote,
        ',' => KeyCode::Comma,
        '.' => KeyCode::Dot,
        '/' => KeyCode::Slash,
        '`' => KeyCode::Grave,
        ' ' => KeyCode::Space,
        '\n' => KeyCode::Enter,
        '\t' => KeyCode::Tab,
        _ => KeyCode::Unknown(c as u32),
    }
}

#[derive(Default)]
pub(crate) struct KanaEncoder;

impl KanaEncoder {
    pub(crate) fn encode(&self, s: &str, mode: InputMode) -> OutputSeq {
        if mode == InputMode::Romaji {
            s.chars()
                .map(|c| {
                    if c.is_ascii_alphabetic() {
                        OutputToken::Key {
                            code: ascii_to_keycode(c),
                            mods: Modifiers::empty(),
                        }
                    } else {
                        OutputToken::Text(c.to_string())
                    }
                })
                .collect()
        } else {
            vec![OutputToken::Text(s.to_string())]
        }
    }
}
