//! LayoutLoader trait + DvorakJ implementation (v1).
//! Shift-JIS (CP932), block parsing, cell compilation to OutputSeq.

use crate::{KeyCode, Modifiers, OutputSeq, OutputToken, InputMode, SpecialKey, layout::Layout};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("Encoding (expected Shift-JIS/CP932): {0}")]
    Encoding(String),
    #[error("Parse: {0}")]
    Parse(String),
    #[error("Unknown trigger name: {0}")]
    UnknownTrigger(String),
    #[error("Schema: {0}")]
    Schema(String),
}

pub trait LayoutLoader: Send + Sync {
    fn load(&self, bytes: &[u8], id: &str) -> Result<Layout, LoadError>;
    fn format_name(&self) -> &'static str;
}

/// DvorakJ-style loader (the only one for v1 per plan).
/// Reference: DvorakJ txt files, Shift-JIS, -option-input, [base], {name}[...], {}{}[combo]
pub struct DvorakJLayoutLoader {
    /// Bundled kana->romaji table for romaji mode (populated later)
    kana_encoder: KanaEncoder,
}

impl DvorakJLayoutLoader {
    pub fn new() -> Self {
        Self {
            kana_encoder: KanaEncoder::default(),
        }
    }
}

impl Default for DvorakJLayoutLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutLoader for DvorakJLayoutLoader {
    fn format_name(&self) -> &'static str { "dvorakj" }

    fn load(&self, bytes: &[u8], id: &str) -> Result<Layout, LoadError> {
        let text = encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned();
        let stripped = strip_comments(&text);
        parse_dvorakj(&stripped, id, &self.kana_encoder)
    }
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if c == '/' && it.peek() == Some(&'*') {
            it.next();
            // skip to */
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

fn parse_dvorakj(text: &str, id: &str, encoder: &KanaEncoder) -> Result<Layout, LoadError> {
    let lines: Vec<&str> = text.lines().collect();
    let mut layout = Layout {
        id: id.to_string(),
        name: id.to_string(),
        input_mode: InputMode::Direct,
        single_map: HashMap::new(),
        layer_maps: HashMap::new(),
        layer_taps: HashMap::new(),
        layer_triggers: std::collections::HashSet::new(),
        simultaneous: vec![],
    };
    let mut layer_triggers: HashMap<String, KeyCode> = HashMap::new();
    // `-shift` is a pre-declared layer (the physical Shift key); it needs no
    // -option-input entry and may appear directly as a `-shift[...]` block (L4).
    layer_triggers.insert("shift".to_string(), KeyCode::ShiftL);
    // Row count of the base layer; layer blocks reuse it to know where the
    // grid ends and the optional trailing tap row begins (L5).
    let mut base_row_count = 0usize;
    let mut i = 0usize;

    // header (first non-blank)
    while i < lines.len() {
        let t = lines[i].trim();
        if !t.is_empty() {
            layout.name = t.to_string();
            i += 1;
            break;
        }
        i += 1;
    }

    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() { i += 1; continue; }

        if line.starts_with("-option-input") {
            if let Some((body, end)) = extract_block(&lines, i) {
                for bl in body {
                    // e.g. "{無変換} | -muhenkan" or "{lalt} | -20"
                    if let Some((lname, trig)) = bl.split_once('|') {
                        // Layer names are written `{name}` in the corpus; normalize to the
                        // bare name so `{name}[...]` block headers match this key (L1).
                        let lname = normalize_layer_name(lname);
                        let trig = trig.trim().trim_start_matches('-');
                        if let Some(kc) = KeyCode::from_dvorakj_name(trig) {
                            layer_triggers.insert(lname, kc);
                        } else if let Ok(num) = trig.parse::<u32>() {
                            // numeric VK reference (decimal) used by some DvorakJ layouts.
                            let kc = keycode_from_numeric_vk(num)
                                .ok_or_else(|| LoadError::UnknownTrigger(format!("numeric {}", num)))?;
                            layer_triggers.insert(lname, kc);
                        } else {
                            return Err(LoadError::UnknownTrigger(trig.to_string()));
                        }
                    }
                }
                i = end + 1;
                continue;
            }
        }

        if line.starts_with('[') {
            // base layer
            if let Some((body, end)) = extract_block(&lines, i) {
                base_row_count = body.len();
                let grid = parse_grid(&body, encoder, InputMode::Direct, 0)?;
                layout.single_map = grid;
                i = end + 1;
                continue;
            }
        }

        // Layer / combo blocks: `{name}[`, `{a}{b}[` (combo), or bare `-shift[`.
        if line.starts_with('{') || (line.starts_with('-') && !line.starts_with("-option-input")) {
            if let Some((body, end)) = extract_block(&lines, i) {
                let names = parse_block_layer_names(line);
                if names.is_empty() { i += 1; continue; }
                let mut layer_ks: Vec<KeyCode> =
                    names.iter().filter_map(|n| layer_triggers.get(n).copied()).collect();
                if layer_ks.len() != names.len() {
                    // A name in the block header was never declared. Fail fast
                    // (NFR-4) rather than silently dropping the layer.
                    let missing: Vec<&String> =
                        names.iter().filter(|n| !layer_triggers.contains_key(*n)).collect();
                    return Err(LoadError::UnknownTrigger(format!("layer name(s) {:?}", missing)));
                }
                layer_ks.sort_by_key(|k| key_sort(*k));

                // Split an optional trailing tap row (sparse: a `{name}` marker
                // or a 1-2 cell row like `、|`) from the grid rows (L5).
                let (grid_body, tap_cell) = split_tap_row(&body);
                // Layer grids are bottom-aligned to the physical rows: corpus
                // layers may omit the top (number) row, never the bottom, so
                // map the last `grid_body.len()` physical rows (L3/L5).
                let total_rows = base_row_count.max(grid_body.len());
                let offset = total_rows.saturating_sub(grid_body.len());
                let grid = parse_grid(grid_body, encoder, InputMode::Direct, offset)?;

                // Tap output for each layer key in this block: `{name}` means the
                // key emits itself; any other cell is its compiled output. Single
                // layer keys default to emitting themselves if no tap row given.
                for (n, &kc) in names.iter().zip(layer_ks.iter()) {
                    let tap_seq = match &tap_cell {
                        Some(cell) if is_self_marker(cell, &names) => {
                            vec![OutputToken::Key { code: kc, mods: Modifiers::empty() }]
                        }
                        Some(cell) => compile_cell(cell, InputMode::Direct, encoder)?,
                        None if names.len() == 1 => {
                            vec![OutputToken::Key { code: kc, mods: Modifiers::empty() }]
                        }
                        None => vec![],
                    };
                    if !tap_seq.is_empty() {
                        layout.layer_taps.entry(kc).or_insert(tap_seq);
                    }
                    let _ = n;
                }

                layout.layer_maps.insert(layer_ks.clone(), grid);
                for k in layer_ks { layout.layer_triggers.insert(k); }
                i = end + 1;
                continue;
            }
        }

        i += 1;
    }

    // populate layer_triggers set from the map we built
    for kc in layer_triggers.values() {
        layout.layer_triggers.insert(*kc);
    }
    Ok(layout)
}

fn key_sort(k: KeyCode) -> u16 {
    // stable small int for sorting layer vecs (manual because KeyCode has data variant)
    match k {
        KeyCode::Space => 1,
        KeyCode::ShiftL => 2,
        KeyCode::ShiftR => 3,
        KeyCode::CtrlL => 4,
        KeyCode::CtrlR => 5,
        KeyCode::AltL => 6,
        KeyCode::AltR => 7,
        KeyCode::MetaL => 8,
        KeyCode::MetaR => 9,
        KeyCode::Muhenkan => 10,
        KeyCode::Henkan => 11,
        KeyCode::KanaKatakana => 12,
        KeyCode::HankakuZenkaku => 13,
        KeyCode::Yen => 14,
        KeyCode::Caret => 15,
        KeyCode::Colon => 16,
        KeyCode::AtSign => 17,
        KeyCode::Unknown(_) => 200,
        _ => 100,
    }
}

/// Extract a bracketed block starting at `idx` (the line containing `[`).
/// Content after `[` on the opener line is part of the body, so single-line
/// blocks like `-option-input[ space | -space ]` parse correctly (the
/// embedded fallback layout in hook/windows.rs relies on this).
fn extract_block(lines: &[&str], mut idx: usize) -> Option<(Vec<String>, usize)> {
    while idx < lines.len() && !lines[idx].contains('[') {
        idx += 1;
    }
    if idx >= lines.len() {
        return None;
    }
    let mut body = vec![];
    let opener = lines[idx];
    let after = &opener[opener.find('[').unwrap() + 1..];

    // Single-line block: `...[ content ]`
    if let Some(close) = after.rfind(']') {
        let inner = after[..close].trim();
        if !inner.is_empty() {
            body.push(inner.to_string());
        }
        return Some((body, idx));
    }
    let t = after.trim();
    if !t.is_empty() {
        body.push(t.to_string());
    }
    idx += 1;
    while idx < lines.len() {
        let t = lines[idx].trim();
        // Grid rows always contain `|`, so a bare `]` (or `x]` without `|`)
        // is unambiguously the terminator.
        if t == "]" || (t.ends_with(']') && !t.contains('|')) {
            let before = t.trim_end_matches(']').trim();
            if !before.is_empty() {
                body.push(before.to_string());
            }
            return Some((body, idx));
        }
        // Skip blank lines. Row alignment does not depend on them (layer grids
        // are bottom-aligned to the physical rows in parse loop), which avoids
        // phantom rows from comment lines stripped to whitespace.
        if !t.is_empty() {
            body.push(t.to_string());
        }
        idx += 1;
    }
    Some((body, idx))
}

/// Strip the surrounding `{...}` (if present) and trim, yielding the bare
/// layer name used as the lookup key. `{無変換}` and `無変換` both normalize
/// to `無変換`, so -option-input declarations and `{name}[...]` block
/// headers agree (L1).
fn normalize_layer_name(raw: &str) -> String {
    let t = raw.trim();
    t.strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .unwrap_or(t)
        .trim()
        .to_string()
}

/// Parse the layer names from a block header line. Handles both the
/// brace form `{name1}{name2}[` (named/combo layers) and the bare
/// `-shift[` form (pre-declared physical Shift, L4).
fn parse_block_layer_names(starter: &str) -> Vec<String> {
    let head = match starter.find('[') {
        Some(pos) => &starter[..pos],
        None => starter,
    };
    let head = head.trim();
    if head.starts_with('-') {
        // `-shift` -> ["shift"]
        let name = head.trim_start_matches('-').trim();
        return if name.is_empty() { vec![] } else { vec![name.to_string()] };
    }
    let mut names = vec![];
    let mut rest = head;
    while let Some(pos) = rest.find('{') {
        if let Some(end) = rest[pos + 1..].find('}') {
            let name = rest[pos + 1..pos + 1 + end].trim();
            if !name.is_empty() {
                names.push(name.to_string());
            }
            rest = &rest[pos + 1 + end + 1..];
        } else {
            break;
        }
    }
    names
}

/// First non-empty `|`-separated cell of a row (used for tap rows like `、|`).
fn first_cell(row: &str) -> Option<String> {
    row.split('|')
        .map(str::trim)
        .find(|c| !c.is_empty())
        .map(|c| c.to_string())
}

/// True if a tap cell is a `{name}` self-marker matching one of the block's
/// layer names (meaning: the layer key, tapped alone, emits itself).
fn is_self_marker(cell: &str, names: &[String]) -> bool {
    let inner = normalize_layer_name(cell);
    cell.starts_with('{') && cell.ends_with('}') && names.iter().any(|n| *n == inner)
}

/// Map a decimal VK reference (DvorakJ `-NN` triggers) to a canonical KeyCode.
/// Only the values observed in the bundled corpus are mapped; anything else
/// is an explicit load error (NFR-4 fail-fast) so the name table gets updated
/// rather than silently dropping a layer.
fn keycode_from_numeric_vk(num: u32) -> Option<KeyCode> {
    // DvorakJ writes these as decimal VK numbers.
    match num {
        20 => Some(KeyCode::CapsLock),     // VK_CAPITAL (0x14)
        21 => Some(KeyCode::KanaKatakana), // VK_KANA    (0x15)
        28 => Some(KeyCode::Henkan),       // VK_CONVERT (0x1C)
        29 => Some(KeyCode::Muhenkan),     // VK_NONCONVERT (0x1D)
        _ => None,
    }
}

/// Compile a grid body into a physical-key -> output map. `row_offset` shifts
/// body row 0 to physical row `row_offset` (0 for the base layer; >0 for a
/// bottom-aligned layer that omits upper physical rows).
fn parse_grid(body: &[String], encoder: &KanaEncoder, mode: InputMode, row_offset: usize) -> Result<HashMap<KeyCode, OutputSeq>, LoadError> {
    let mut out = HashMap::new();
    for (r, line) in body.iter().enumerate() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        let phys = jis_physical_row(r + row_offset);
        if phys.is_empty() { continue; }
        let n = std::cmp::min(cells.len(), phys.len());
        for i in 0..n {
            let cell = cells[i];
            if cell.is_empty() || cell == "@@@" { continue; }
            let seq = compile_cell(cell, mode, encoder)?;
            if !seq.is_empty() {
                out.insert(phys[i], seq);
            }
        }
    }
    Ok(out)
}

/// Separate a trailing tap row from grid rows. The tap row is a sparse final
/// row: a `{name}` self-marker (no `|`) or a 1-2 cell row such as `、|`. Grid
/// rows are full keyboard rows with many cells, so this is unambiguous.
fn split_tap_row(body: &[String]) -> (&[String], Option<String>) {
    if body.len() >= 2 {
        if let Some(last) = body.last() {
            let cell_count = last.split('|').filter(|c| !c.trim().is_empty()).count();
            if cell_count <= 2 {
                return (&body[..body.len() - 1], first_cell(last));
            }
        }
    }
    (body, None)
}

/// Physical key for a (row, column) position in a DvorakJ grid, matching the
/// JIS / OADG 109A layout the corpus targets (L3):
///   row 0: 1 2 3 4 5 6 7 8 9 0 - ^ ¥
///   row 1: Q W E R T Y U I O P @ [
///   row 2: A S D F G H J K L ; : ]
///   row 3: Z X C V B N M , . / \
fn jis_physical_row(row: usize) -> &'static [KeyCode] {
    match row {
        0 => &[KeyCode::Num1, KeyCode::Num2, KeyCode::Num3, KeyCode::Num4, KeyCode::Num5, KeyCode::Num6, KeyCode::Num7, KeyCode::Num8, KeyCode::Num9, KeyCode::Num0, KeyCode::Minus, KeyCode::Caret, KeyCode::Yen],
        1 => &[KeyCode::Q, KeyCode::W, KeyCode::E, KeyCode::R, KeyCode::T, KeyCode::Y, KeyCode::U, KeyCode::I, KeyCode::O, KeyCode::P, KeyCode::AtSign, KeyCode::LBracket],
        2 => &[KeyCode::A, KeyCode::S, KeyCode::D, KeyCode::F, KeyCode::G, KeyCode::H, KeyCode::J, KeyCode::K, KeyCode::L, KeyCode::Semicolon, KeyCode::Colon, KeyCode::RBracket],
        3 => &[KeyCode::Z, KeyCode::X, KeyCode::C, KeyCode::V, KeyCode::B, KeyCode::N, KeyCode::M, KeyCode::Comma, KeyCode::Dot, KeyCode::Slash, KeyCode::Backslash],
        _ => &[],
    }
}

fn compile_cell(cell: &str, mode: InputMode, encoder: &KanaEncoder) -> Result<OutputSeq, LoadError> {
    let c = cell.trim();
    if c.is_empty() || c == "@@@" { return Ok(vec![]); }
    if c.starts_with('{') && c.ends_with('}') {
        let inner = &c[1..c.len()-1].to_lowercase();
        let tok = match inner.as_str() {
            "bs" | "backspace" => OutputToken::Named(SpecialKey::Backspace),
            "enter" | "return" => OutputToken::Named(SpecialKey::Enter),
            "tab" => OutputToken::Named(SpecialKey::Tab),
            "esc" | "escape" => OutputToken::Named(SpecialKey::Escape),
            "left" => OutputToken::Named(SpecialKey::Left),
            "right" => OutputToken::Named(SpecialKey::Right),
            "up" => OutputToken::Named(SpecialKey::Up),
            "down" => OutputToken::Named(SpecialKey::Down),
            "space" => OutputToken::Key { code: KeyCode::Space, mods: Modifiers::empty() },
            s if s.len() == 1 => {
                let ch = s.chars().next().unwrap();
                let upper = ch.is_ascii_uppercase();
                OutputToken::Key { code: ascii_to_keycode(ch), mods: if upper { Modifiers::SHIFT } else { Modifiers::empty() } }
            }
            _ => OutputToken::Text(c.to_string()),
        };
        return Ok(vec![tok]);
    }
    // text / multi-char
    if mode == InputMode::Romaji {
        Ok(encoder.encode(c, mode))
    } else {
        let mut seq = vec![];
        for ch in c.chars() {
            if ch.is_ascii_alphanumeric() || " -=\\/[];',.`~!@#$%^&*()_+|{}:\"<>?".contains(ch) {
                let upper = ch.is_ascii_uppercase();
                seq.push(OutputToken::Key { code: ascii_to_keycode(ch), mods: if upper { Modifiers::SHIFT } else { Modifiers::empty() } });
            } else {
                seq.push(OutputToken::Text(ch.to_string()));
            }
        }
        Ok(seq)
    }
}

// (toy removed; real DvorakJ parser now active)

/// Placeholder kana encoder. Real table from DvorakJ or standard romaji.
#[derive(Default)]
struct KanaEncoder {
    // table: HashMap<String, OutputSeq>,
}

impl KanaEncoder {
    fn encode(&self, s: &str, mode: InputMode) -> OutputSeq {
        // For now, if romaji, naive: each char as direct key if ASCII.
        // Later: full table for あ->a, い->i, きゃ->kya etc.
        if mode == InputMode::Romaji {
            s.chars().filter_map(|c| {
                // very naive ASCII passthrough for bootstrap
                if c.is_ascii_alphabetic() {
                    Some(OutputToken::Key { code: ascii_to_keycode(c), mods: Modifiers::empty() })
                } else {
                    Some(OutputToken::Text(c.to_string()))
                }
            }).collect()
        } else {
            vec![OutputToken::Text(s.to_string())]
        }
    }
}

fn ascii_to_keycode(c: char) -> KeyCode {
    match c.to_ascii_lowercase() {
        'a' => KeyCode::A, 'b' => KeyCode::B, 'c' => KeyCode::C, 'd' => KeyCode::D,
        'e' => KeyCode::E, 'f' => KeyCode::F, 'g' => KeyCode::G, 'h' => KeyCode::H,
        'i' => KeyCode::I, 'j' => KeyCode::J, 'k' => KeyCode::K, 'l' => KeyCode::L,
        'm' => KeyCode::M, 'n' => KeyCode::N, 'o' => KeyCode::O, 'p' => KeyCode::P,
        'q' => KeyCode::Q, 'r' => KeyCode::R, 's' => KeyCode::S, 't' => KeyCode::T,
        'u' => KeyCode::U, 'v' => KeyCode::V, 'w' => KeyCode::W, 'x' => KeyCode::X,
        'y' => KeyCode::Y, 'z' => KeyCode::Z,
        '0' => KeyCode::Num0, '1' => KeyCode::Num1, '2' => KeyCode::Num2, '3' => KeyCode::Num3,
        '4' => KeyCode::Num4, '5' => KeyCode::Num5, '6' => KeyCode::Num6, '7' => KeyCode::Num7,
        '8' => KeyCode::Num8, '9' => KeyCode::Num9,
        '-' => KeyCode::Minus, '=' => KeyCode::Equal, '[' => KeyCode::LBracket, ']' => KeyCode::RBracket,
        '\\' => KeyCode::Backslash, ';' => KeyCode::Semicolon, '\'' => KeyCode::Quote,
        ',' => KeyCode::Comma, '.' => KeyCode::Dot, '/' => KeyCode::Slash, '`' => KeyCode::Grave,
        ' ' => KeyCode::Space, '\n' => KeyCode::Enter, '\t' => KeyCode::Tab,
        _ => KeyCode::Unknown(c as u32),
    }
}
