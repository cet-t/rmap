//! LayoutLoader trait + DvorakJ implementation (v1).
//! Shift-JIS (CP932), block parsing, cell compilation to OutputSeq.

use crate::{KeyCode, KeyboardLayout, Modifiers, OutputSeq, OutputToken, InputMode, SpecialKey, layout::Layout};
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
        // Filename decides the dialect (per app spec): `*.jp.txt` is the
        // genuine DvorakJ corpus (JIS-only, Shift-JIS); `*.en.txt` is this
        // app's own English/US-ANSI spec (UTF-8). Files with neither suffix
        // fall back to `self.keyboard` (Shift-JIS-decoded, for legacy/test
        // samples), per OS-locale detection.
        let (keyboard, text) = if id.ends_with(".en.txt") {
            (KeyboardLayout::Us, String::from_utf8_lossy(bytes).into_owned())
        } else if id.ends_with(".jp.txt") {
            (KeyboardLayout::Jis, encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned())
        } else {
            // Files with no suffix are JIS-format DvorakJ-style layouts
            // (JIS physical rows), but their encoding varies: the bundled
            // toy corpus is plain ASCII Shift-JIS, while user-authored
            // layouts are commonly saved as UTF-8 (with or without a BOM).
            // Try UTF-8 first and fall back to Shift-JIS only if the bytes
            // aren't valid UTF-8.
            let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
            let text = match std::str::from_utf8(bytes) {
                Ok(s) => s.to_string(),
                Err(_) => encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned(),
            };
            (KeyboardLayout::Jis, text)
        };
        let stripped = strip_comments(&text);
        parse_dvorakj(&stripped, id, &self.kana_encoder, keyboard)
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

fn parse_dvorakj(text: &str, id: &str, encoder: &KanaEncoder, keyboard: KeyboardLayout) -> Result<Layout, LoadError> {
    let lines: Vec<&str> = text.lines().collect();
    let mut layout = Layout {
        id: id.to_string(),
        name: id.to_string(),
        input_mode: InputMode::Direct,
        single_map: HashMap::new(),
        layer_maps: HashMap::new(),
        layer_taps: HashMap::new(),
        layer_triggers: std::collections::HashSet::new(),
        combos: HashMap::new(),
        combo_keys: std::collections::HashSet::new(),
        sustained_triggers: std::collections::HashSet::new(),
        simultaneous: vec![],
        keyboard,
    };
    let mut layer_triggers: HashMap<String, KeyCode> = HashMap::new();
    // Names declared via `-option-input` are *sustained* (while-held) layers
    // (SandS / thumb-shift), as opposed to bare scan-code 同時打鍵 chords.
    let mut sustained_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    // `-shift` is a pre-declared layer (the physical Shift key); it needs no
    // -option-input entry and may appear directly as a `-shift[...]` block (L4).
    // It is a sustained modifier layer.
    layer_triggers.insert("shift".to_string(), KeyCode::ShiftL);
    sustained_names.insert("shift".to_string());
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
                            layer_triggers.insert(lname.clone(), kc);
                            sustained_names.insert(lname);
                        } else if let Ok(num) = trig.parse::<u32>() {
                            // numeric VK reference (decimal) used by some DvorakJ layouts.
                            let kc = keycode_from_numeric_vk(num)
                                .ok_or_else(|| LoadError::UnknownTrigger(format!("numeric {}", num)))?;
                            layer_triggers.insert(lname.clone(), kc);
                            sustained_names.insert(lname);
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
                let grid = parse_grid(&body, encoder, InputMode::Direct, 0, keyboard)?;
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
                let mut layer_ks: Vec<KeyCode> = Vec::with_capacity(names.len());
                let mut missing: Vec<String> = vec![];
                for n in &names {
                    if let Some(kc) = layer_triggers.get(n) {
                        layer_ks.push(*kc);
                    } else if let Some(kc) = u32::from_str_radix(n, 16).ok().and_then(keycode_from_scancode) {
                        // Bare `-XX[...]` headers with no prior -option-input
                        // declaration name the trigger by its hex scan code
                        // (L_NEW corpus style); resolve and remember it.
                        layer_triggers.insert(n.clone(), kc);
                        layer_ks.push(kc);
                    } else {
                        missing.push(n.clone());
                    }
                }
                if !missing.is_empty() {
                    // A name in the block header was never declared and isn't a
                    // recognized scan code. Fail fast (NFR-4) rather than
                    // silently dropping the layer.
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
                let grid = parse_grid(grid_body, encoder, InputMode::Direct, offset, keyboard)?;

                // Tap output for each layer key in this block: `{name}` means the
                // key emits itself; any other cell is its compiled output. A
                // single layer key with no tap row defaults to its own base-grid
                // mapping (e.g. on a combo layout, the E/R/U/I/O keys still type
                // their base kana when tapped alone), falling back to emitting
                // the key itself for trigger keys not present in the base grid
                // (e.g. Muhenkan/Henkan/Space).
                for (n, &kc) in names.iter().zip(layer_ks.iter()) {
                    let tap_seq = match &tap_cell {
                        Some(cell) if is_self_marker(cell, &names) => {
                            vec![OutputToken::Key { code: kc, mods: Modifiers::empty() }]
                        }
                        Some(cell) => compile_cell(cell, InputMode::Direct, encoder, keyboard)?,
                        None if names.len() == 1 => {
                            layout.single_map.get(&kc).cloned().unwrap_or_else(|| {
                                vec![OutputToken::Key { code: kc, mods: Modifiers::empty() }]
                            })
                        }
                        None => vec![],
                    };
                    if !tap_seq.is_empty() {
                        layout.layer_taps.entry(kc).or_insert(tap_seq);
                    }
                    let _ = n;
                }

                // A block is *sustained* (while-held layer, SandS) iff every one
                // of its trigger names was declared via `-option-input`; bare
                // scan-code triggers (新下駄) are one-shot 同時打鍵 chords.
                let is_sustained = names.iter().all(|n| sustained_names.contains(n));
                if is_sustained {
                    // Hold-layer semantics: keep the per-content layer map and
                    // mark the trigger(s) as sustained.
                    layout.layer_maps.insert(layer_ks.clone(), grid);
                    for &k in &layer_ks { layout.sustained_triggers.insert(k); }
                } else {
                    // 同時打鍵: each (content key -> output) becomes a chord of
                    // {trigger keys} ∪ {content key}. Every participating key is a
                    // combo key (defers its solo output to allow chord detection).
                    for (&content, out) in &grid {
                        let mut chord = layer_ks.clone();
                        chord.push(content);
                        crate::layout::canon_sort(&mut chord);
                        chord.dedup();
                        layout.combos.entry(chord).or_insert_with(|| out.clone());
                        layout.combo_keys.insert(content);
                    }
                    for &k in &layer_ks { layout.combo_keys.insert(k); }
                    // Keep the layer map too (harmless; lets diagnostics/tests
                    // still inspect chord blocks by trigger set).
                    layout.layer_maps.insert(layer_ks.clone(), grid);
                }
                for k in layer_ks { layout.layer_triggers.insert(k); }
                i = end + 1;
                continue;
            }
        }

        i += 1;
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
        // `-shift` -> ["shift"]; `-17-18` (hex scan-code combo) -> ["17", "18"]
        let rest = head.trim_start_matches('-').trim();
        return rest.split('-').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
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

/// Map a hex PC/AT Set-1 scan code to a canonical KeyCode. Used to resolve
/// bare `-XX[...]` / `-XX-YY[...]` layer-block headers that reference a key
/// by scan code without a prior `-option-input` declaration (seen in
/// hand-written "新下駄配列"-style corpora). Covers the alphanumeric block
/// plus the extra 102-key JIS key (0x73, "\_"/Yen row).
fn keycode_from_scancode(code: u32) -> Option<KeyCode> {
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

/// Compile a grid body into a physical-key -> output map. `row_offset` shifts
/// body row 0 to physical row `row_offset` (0 for the base layer; >0 for a
/// bottom-aligned layer that omits upper physical rows).
fn parse_grid(body: &[String], encoder: &KanaEncoder, mode: InputMode, row_offset: usize, keyboard: KeyboardLayout) -> Result<HashMap<KeyCode, OutputSeq>, LoadError> {
    let mut out = HashMap::new();
    for (r, line) in body.iter().enumerate() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        let phys = physical_row(r + row_offset, keyboard);
        if phys.is_empty() { continue; }
        let n = std::cmp::min(cells.len(), phys.len());
        for i in 0..n {
            let cell = cells[i];
            if cell.is_empty() || cell == "@@@" { continue; }
            let seq = compile_cell(cell, mode, encoder, keyboard)?;
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
            // A tap row is a short standalone row (1-2 cells total, e.g. `, |`
            // or `{space}`), not a full physical-key row that merely has most
            // of its cells empty (e.g. a combo layer's row with only 1-2
            // mappings on an otherwise-empty 11/12-column row).
            let total_cells = last.split('|').count();
            let non_empty = last.split('|').filter(|c| !c.trim().is_empty()).count();
            if total_cells <= 2 && non_empty >= 1 {
                return (&body[..body.len() - 1], first_cell(last));
            }
        }
    }
    (body, None)
}

/// Physical key for a (row, column) position in a DvorakJ grid.
///
/// JIS / OADG 109A (`KeyboardLayout::Jis`), the layout the corpus targets (L3):
///   row 0: 1 2 3 4 5 6 7 8 9 0 - ^ ¥
///   row 1: Q W E R T Y U I O P @ [
///   row 2: A S D F G H J K L ; : ]
///   row 3: Z X C V B N M , . / \
///
/// US/ANSI 104 (`KeyboardLayout::Us`): used for `*.en.txt` own-spec layouts
/// (see `parse_dvorakj`'s caller in `load`) and as the fallback for files
/// with neither `.jp.txt` nor `.en.txt` suffix. Grid rows that are shorter
/// than these tables simply leave the trailing physical keys unmapped
/// (`parse_grid` truncates to `min(cells.len(), phys.len())`).
///   row 0: 1 2 3 4 5 6 7 8 9 0 - = `
///   row 1: Q W E R T Y U I O P [ ] \
///   row 2: A S D F G H J K L ; '
///   row 3: Z X C V B N M , . /
///
/// Note: Grave is placed *last* in row 0 (not first), so the digit row
/// (`数字段`, 1..0) lines up at columns 0..9 like the JIS table above —
/// authors don't need to shift every digit cell right by one to make room
/// for the backtick key.
fn physical_row(row: usize, keyboard: KeyboardLayout) -> &'static [KeyCode] {
    match (keyboard, row) {
        (KeyboardLayout::Jis, 0) => &[KeyCode::Num1, KeyCode::Num2, KeyCode::Num3, KeyCode::Num4, KeyCode::Num5, KeyCode::Num6, KeyCode::Num7, KeyCode::Num8, KeyCode::Num9, KeyCode::Num0, KeyCode::Minus, KeyCode::Caret, KeyCode::Yen],
        (KeyboardLayout::Jis, 1) => &[KeyCode::Q, KeyCode::W, KeyCode::E, KeyCode::R, KeyCode::T, KeyCode::Y, KeyCode::U, KeyCode::I, KeyCode::O, KeyCode::P, KeyCode::AtSign, KeyCode::LBracket],
        (KeyboardLayout::Jis, 2) => &[KeyCode::A, KeyCode::S, KeyCode::D, KeyCode::F, KeyCode::G, KeyCode::H, KeyCode::J, KeyCode::K, KeyCode::L, KeyCode::Semicolon, KeyCode::Colon, KeyCode::RBracket],
        (KeyboardLayout::Jis, 3) => &[KeyCode::Z, KeyCode::X, KeyCode::C, KeyCode::V, KeyCode::B, KeyCode::N, KeyCode::M, KeyCode::Comma, KeyCode::Dot, KeyCode::Slash, KeyCode::Backslash],

        (KeyboardLayout::Us, 0) => &[KeyCode::Num1, KeyCode::Num2, KeyCode::Num3, KeyCode::Num4, KeyCode::Num5, KeyCode::Num6, KeyCode::Num7, KeyCode::Num8, KeyCode::Num9, KeyCode::Num0, KeyCode::Minus, KeyCode::Equal, KeyCode::Grave],
        (KeyboardLayout::Us, 1) => &[KeyCode::Q, KeyCode::W, KeyCode::E, KeyCode::R, KeyCode::T, KeyCode::Y, KeyCode::U, KeyCode::I, KeyCode::O, KeyCode::P, KeyCode::LBracket, KeyCode::RBracket, KeyCode::Backslash],
        (KeyboardLayout::Us, 2) => &[KeyCode::A, KeyCode::S, KeyCode::D, KeyCode::F, KeyCode::G, KeyCode::H, KeyCode::J, KeyCode::K, KeyCode::L, KeyCode::Semicolon, KeyCode::Quote],
        (KeyboardLayout::Us, 3) => &[KeyCode::Z, KeyCode::X, KeyCode::C, KeyCode::V, KeyCode::B, KeyCode::N, KeyCode::M, KeyCode::Comma, KeyCode::Dot, KeyCode::Slash],

        (_, _) => &[],
    }
}

fn compile_cell(cell: &str, mode: InputMode, encoder: &KanaEncoder, keyboard: KeyboardLayout) -> Result<OutputSeq, LoadError> {
    let c = cell.trim();
    if c.is_empty() || c == "@@@" { return Ok(vec![]); }
    // Pure kana/text cells (no `{...}` function-key tokens) go through the
    // romaji encoder as before.
    if mode == InputMode::Romaji && !c.contains('{') {
        return Ok(encoder.encode(c, mode));
    }
    // Tokenize into literal characters and `{...}` function-key tokens, so
    // mixed cells like "！{enter}" or "「」{enter}{left}" compile to the
    // literal text followed by the named key(s) instead of being typed as a
    // literal "{enter}" string.
    let mut seq = vec![];
    let mut chars = c.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut inner = String::new();
            let mut closed = false;
            while let Some(nc) = chars.next() {
                if nc == '}' { closed = true; break; }
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

/// Compile the contents of a `{...}` cell token (without the braces) to a
/// single output token: named function keys, `{pipe}`/`{bar}` for the column
/// separator, single-char shorthand (`{a}` -> 'a'), or unrecognized names
/// passed through as literal `{name}` text.
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
        "space" => OutputToken::Key { code: KeyCode::Space, mods: Modifiers::empty() },
        // `|` can't appear literally in a grid cell (it's the column separator),
        // so the corpus spells it `{pipe}`/`{bar}`.
        "pipe" | "bar" => OutputToken::Text("|".to_string()),
        _ if s.len() == 1 => key_or_text(s.chars().next().unwrap(), keyboard),
        _ => OutputToken::Text(format!("{{{}}}", inner)),
    }
}

/// Compile one output char to a keystroke when `ascii_to_keycode` knows a
/// physical key for it (with SHIFT for uppercase letters), otherwise fall
/// back to direct Unicode injection (`OutputToken::Text`). The Unicode path
/// works regardless of the active OS keyboard layout, so symbols that have no
/// dedicated `KeyCode` (e.g. `~ ! @ # $ % ^ & * ( ) _ + { } | : " < > ?`)
/// still get typed instead of being silently dropped.
///
/// For `KeyboardLayout::Us` (`.en.txt`) punctuation goes via Unicode
/// injection: a `Key{code, mods}` keystroke is translated to a character by
/// whatever physical keyboard layout is actually active in Windows (often
/// JIS for this app's users), so e.g. `;`+Shift would type `+` instead of
/// `:`. Unicode injection sidesteps the active OS layout entirely, so
/// `.en.txt` punctuation output always matches its own-spec definition
/// regardless of OS layout.
///
/// Letters and digits stay on the `Key{code, mods}` path even for `Us`:
/// their VK codes (and Shift behavior) are identical across JIS/US layouts,
/// and going through a real VK keystroke lets concurrently-held modifiers
/// (Ctrl, Alt, Win) combine on the OS side -- Unicode injection ignores
/// modifier state entirely, which would turn e.g. Ctrl+A into a literal "a".
fn key_or_text(ch: char, keyboard: KeyboardLayout) -> OutputToken {
    if keyboard == KeyboardLayout::Us && !ch.is_ascii_alphanumeric() {
        return OutputToken::Text(ch.to_string());
    }
    let code = ascii_to_keycode(ch);
    if matches!(code, KeyCode::Unknown(_)) {
        OutputToken::Text(ch.to_string())
    } else {
        let mods = if ch.is_ascii_uppercase() { Modifiers::SHIFT } else { Modifiers::empty() };
        OutputToken::Key { code, mods }
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
