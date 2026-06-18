//! DvorakJ numeric VK / scan-code → KeyCode conversion tables.

use rmap_core::KeyCode;

pub(crate) fn keycode_from_numeric_vk(num: u32) -> Option<KeyCode> {
    match num {
        20 => Some(KeyCode::CapsLock),     // VK_CAPITAL (0x14)
        21 => Some(KeyCode::KanaKatakana), // VK_KANA    (0x15)
        28 => Some(KeyCode::Henkan),       // VK_CONVERT (0x1C)
        29 => Some(KeyCode::Muhenkan),     // VK_NONCONVERT (0x1D)
        _ => None,
    }
}

pub(crate) fn keycode_from_scancode(code: u32) -> Option<KeyCode> {
    match code {
        0x02 => Some(KeyCode::Num1), 0x03 => Some(KeyCode::Num2), 0x04 => Some(KeyCode::Num3),
        0x05 => Some(KeyCode::Num4), 0x06 => Some(KeyCode::Num5), 0x07 => Some(KeyCode::Num6),
        0x08 => Some(KeyCode::Num7), 0x09 => Some(KeyCode::Num8), 0x0A => Some(KeyCode::Num9),
        0x0B => Some(KeyCode::Num0),
        0x0C => Some(KeyCode::Minus), 0x0D => Some(KeyCode::Equal),
        0x10 => Some(KeyCode::Q), 0x11 => Some(KeyCode::W), 0x12 => Some(KeyCode::E),
        0x13 => Some(KeyCode::R), 0x14 => Some(KeyCode::T), 0x15 => Some(KeyCode::Y),
        0x16 => Some(KeyCode::U), 0x17 => Some(KeyCode::I), 0x18 => Some(KeyCode::O),
        0x19 => Some(KeyCode::P),
        0x1A => Some(KeyCode::LBracket), 0x1B => Some(KeyCode::RBracket),
        0x1E => Some(KeyCode::A), 0x1F => Some(KeyCode::S), 0x20 => Some(KeyCode::D),
        0x21 => Some(KeyCode::F), 0x22 => Some(KeyCode::G), 0x23 => Some(KeyCode::H),
        0x24 => Some(KeyCode::J), 0x25 => Some(KeyCode::K), 0x26 => Some(KeyCode::L),
        0x27 => Some(KeyCode::Semicolon), 0x28 => Some(KeyCode::Quote),
        0x2B => Some(KeyCode::Backslash),
        0x2C => Some(KeyCode::Z), 0x2D => Some(KeyCode::X), 0x2E => Some(KeyCode::C),
        0x2F => Some(KeyCode::V), 0x30 => Some(KeyCode::B), 0x31 => Some(KeyCode::N),
        0x32 => Some(KeyCode::M),
        0x33 => Some(KeyCode::Comma), 0x34 => Some(KeyCode::Dot), 0x35 => Some(KeyCode::Slash),
        0x39 => Some(KeyCode::Space),
        0x73 => Some(KeyCode::Backslash), // 102-key JIS extra ("\_") key
        _ => None,
    }
}
