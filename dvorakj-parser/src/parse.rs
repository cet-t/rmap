//! Main DvorakJ layout parser: orchestrates block, grid, cell, and keymap
//! modules to build a [`Layout`] from pre-processed text lines.

use rmap_core::{
    KeyCode, KeyboardLayout, Modifiers, OutputToken, InputMode,
    layout::Layout,
    loader::LoadError,
};
use std::collections::HashMap;

use crate::block::{extract_block, normalize_layer_name, parse_block_layer_names, is_self_marker, split_tap_row, key_sort};
use crate::cell::{compile_cell, KanaEncoder};
use crate::grid::parse_grid;
use crate::keymap::{keycode_from_numeric_vk, keycode_from_scancode};

pub(crate) fn parse_dvorakj(text: &str, id: &str, encoder: &KanaEncoder, keyboard: KeyboardLayout) -> Result<Layout, LoadError> {
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
    let mut sustained_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    layer_triggers.insert("shift".to_string(), KeyCode::ShiftL);
    sustained_names.insert("shift".to_string());
    let mut base_row_count = 0usize;
    let mut i = 0usize;

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
                    if let Some((lname, trig)) = bl.split_once('|') {
                        let lname = normalize_layer_name(lname);
                        let trig = trig.trim().trim_start_matches('-');
                        if let Some(kc) = KeyCode::from_dvorakj_name(trig) {
                            layer_triggers.insert(lname.clone(), kc);
                            sustained_names.insert(lname);
                        } else if let Ok(num) = trig.parse::<u32>() {
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
            if let Some((body, end)) = extract_block(&lines, i) {
                base_row_count = body.len();
                let grid = parse_grid(&body, encoder, InputMode::Direct, 0, keyboard)?;
                layout.single_map = grid;
                i = end + 1;
                continue;
            }
        }

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
                        layer_triggers.insert(n.clone(), kc);
                        layer_ks.push(kc);
                    } else {
                        missing.push(n.clone());
                    }
                }
                if !missing.is_empty() {
                    return Err(LoadError::UnknownTrigger(format!("layer name(s) {:?}", missing)));
                }
                layer_ks.sort_by_key(|k| key_sort(*k));

                let (grid_body, tap_cell) = split_tap_row(&body);
                let total_rows = base_row_count.max(grid_body.len());
                let offset = total_rows.saturating_sub(grid_body.len());
                let grid = parse_grid(grid_body, encoder, InputMode::Direct, offset, keyboard)?;

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

                let is_sustained = names.iter().all(|n| sustained_names.contains(n));
                if is_sustained {
                    layout.layer_maps.insert(layer_ks.clone(), grid);
                    for &k in &layer_ks { layout.sustained_triggers.insert(k); }
                } else {
                    for (&content, out) in &grid {
                        let mut chord = layer_ks.clone();
                        chord.push(content);
                        rmap_core::layout::canon_sort(&mut chord);
                        chord.dedup();
                        layout.combos.entry(chord).or_insert_with(|| out.clone());
                        layout.combo_keys.insert(content);
                    }
                    for &k in &layer_ks { layout.combo_keys.insert(k); }
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
