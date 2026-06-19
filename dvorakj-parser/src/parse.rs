//! Main DvorakJ layout parser: orchestrates block, grid, cell, and keymap
//! modules to build a [`Layout`] from pre-processed text lines.

use rmap_core::{
    layout::{Layout, LayoutMode},
    loader::LoadError,
    InputMode, KeyCode, KeyboardLayout, Modifiers, OutputToken,
};
use std::collections::HashMap;

use crate::block::{
    extract_block, extract_block_from_last_bracket, is_self_marker, key_sort, normalize_layer_name,
    parse_block_layer_names, split_tap_row,
};
use crate::cell::{compile_cell, KanaEncoder};
use crate::grid::parse_grid;
use crate::keymap::keycode_from_scancode;

fn detect_mode(first_line: &str) -> LayoutMode {
    let has_sequential = first_line.contains('順');
    let has_simultaneous = first_line.contains("同時");
    match (has_sequential, has_simultaneous) {
        (true, true) => LayoutMode::Mixed,
        (false, true) => LayoutMode::Simultaneous,
        (true, false) => LayoutMode::Sequential,
        (false, false) => LayoutMode::Legacy,
    }
}

/// Detect `[name],[name][` bracket-named layer blocks: the part before the
/// last `[` contains at least one `]`, meaning there are bracket-enclosed names.
fn is_bracket_named_block(line: &str) -> bool {
    if let Some(last_open) = line.rfind('[') {
        last_open > 0 && line[..last_open].contains(']')
    } else {
        false
    }
}

/// Parse bracket-delimited layer names from a header like `[d],[k]`.
fn parse_bracket_names(header: &str) -> Vec<String> {
    let mut names = vec![];
    let mut rest = header;
    while let Some(open) = rest.find('[') {
        if let Some(close) = rest[open + 1..].find(']') {
            let name = rest[open + 1..open + 1 + close].trim();
            if !name.is_empty() {
                names.push(name.to_string());
            }
            rest = &rest[open + 1 + close + 1..];
        } else {
            break;
        }
    }
    names
}

fn resolve_trigger(trig: &str) -> Result<KeyCode, LoadError> {
    if let Some(kc) = KeyCode::from_dvorakj_name(trig) {
        return Ok(kc);
    }
    if let Ok(code) = u32::from_str_radix(trig, 16) {
        if let Some(kc) = keycode_from_scancode(code) {
            return Ok(kc);
        }
    }
    Err(LoadError::UnknownTrigger(trig.to_string()))
}

pub(crate) fn parse_dvorakj(
    text: &str,
    id: &str,
    encoder: &KanaEncoder,
    keyboard: KeyboardLayout,
) -> Result<Layout, LoadError> {
    let lines: Vec<&str> = text.lines().collect();
    let mut layout = Layout {
        id: id.to_string(),
        name: id.to_string(),
        mode: LayoutMode::default(),
        input_mode: InputMode::Direct,
        single_map: HashMap::new(),
        layer_maps: HashMap::new(),
        layer_taps: HashMap::new(),
        layer_triggers: std::collections::HashSet::new(),
        combos: HashMap::new(),
        combo_keys: std::collections::HashSet::new(),
        sustained_triggers: std::collections::HashSet::new(),
        prefix_maps: HashMap::new(),
        prefix_triggers: std::collections::HashSet::new(),
        simultaneous: vec![],
        keyboard,
    };
    let mut layer_triggers: HashMap<String, KeyCode> = HashMap::new();
    let mut sustained_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    layer_triggers.insert("shift".to_string(), KeyCode::ShiftL);
    sustained_names.insert("shift".to_string());
    let mut base_row_count = 0usize;
    let mut i = 0usize;

    // Parse first non-empty line as layout name and detect mode.
    while i < lines.len() {
        let t = lines[i].trim();
        if !t.is_empty() {
            layout.name = t.to_string();
            layout.mode = detect_mode(t);
            i += 1;
            break;
        }
        i += 1;
    }

    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() {
            i += 1;
            continue;
        }

        if line.starts_with("-option-input") {
            if let Some((body, end)) = extract_block(&lines, i) {
                for bl in body {
                    if let Some((lname, trig_raw)) = bl.split_once('|') {
                        let lname = normalize_layer_name(lname);
                        let trig = trig_raw.trim().trim_start_matches('-');
                        let kc = resolve_trigger(trig)?;
                        layer_triggers.insert(lname.clone(), kc);
                        // Legacy/Simultaneous: -option-input triggers are sustained (SandS).
                        // Sequential/Mixed: they are prefix triggers, not sustained.
                        match layout.mode {
                            LayoutMode::Legacy | LayoutMode::Simultaneous => {
                                sustained_names.insert(lname);
                            }
                            LayoutMode::Sequential | LayoutMode::Mixed => {
                                // Don't add to sustained_names — will be routed as prefix.
                            }
                        }
                    }
                }
                i = end + 1;
                continue;
            }
        }

        // Bracket-named layer blocks: `[d],[k][...]` etc.
        if line.starts_with('[') && is_bracket_named_block(line) {
            if let Some((body, end)) = extract_block_from_last_bracket(&lines, i) {
                let last_open = line.rfind('[').unwrap();
                let header = &line[..last_open];
                let names = parse_bracket_names(header);
                if !names.is_empty() {
                    let mut layer_ks: Vec<KeyCode> = Vec::with_capacity(names.len());
                    let mut missing: Vec<String> = vec![];
                    for n in &names {
                        if let Some(kc) = layer_triggers.get(n) {
                            layer_ks.push(*kc);
                        } else if let Some(kc) = u32::from_str_radix(n, 16)
                            .ok()
                            .and_then(keycode_from_scancode)
                        {
                            layer_triggers.insert(n.clone(), kc);
                            layer_ks.push(kc);
                        } else {
                            missing.push(n.clone());
                        }
                    }
                    if !missing.is_empty() {
                        return Err(LoadError::UnknownTrigger(format!(
                            "layer name(s) {:?}",
                            missing
                        )));
                    }
                    layer_ks.sort_by_key(|k| key_sort(*k));

                    let (grid_body, tap_cell) = split_tap_row(&body);
                    let total_rows = base_row_count.max(grid_body.len());
                    let offset = total_rows.saturating_sub(grid_body.len());
                    let grid = parse_grid(grid_body, encoder, InputMode::Direct, offset, keyboard)?;

                    for (n, &kc) in names.iter().zip(layer_ks.iter()) {
                        let tap_seq = match &tap_cell {
                            Some(cell) if is_self_marker(cell, &names) => {
                                vec![OutputToken::Key {
                                    code: kc,
                                    mods: Modifiers::empty(),
                                }]
                            }
                            Some(cell) => compile_cell(cell, InputMode::Direct, encoder, keyboard)?,
                            None if names.len() == 1 => {
                                layout.single_map.get(&kc).cloned().unwrap_or_else(|| {
                                    vec![OutputToken::Key {
                                        code: kc,
                                        mods: Modifiers::empty(),
                                    }]
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
                    let route = determine_route(layout.mode, is_sustained, false);

                    match route {
                        BlockRoute::Sustained => {
                            layout.layer_maps.insert(layer_ks.clone(), grid);
                            for &k in &layer_ks {
                                layout.sustained_triggers.insert(k);
                            }
                        }
                        BlockRoute::Combo => {
                            for (&content, out) in &grid {
                                let mut chord = layer_ks.clone();
                                chord.push(content);
                                rmap_core::layout::canon_sort(&mut chord);
                                chord.dedup();
                                layout.combos.entry(chord).or_insert_with(|| out.clone());
                                layout.combo_keys.insert(content);
                            }
                            for &k in &layer_ks {
                                layout.combo_keys.insert(k);
                            }
                            layout.layer_maps.insert(layer_ks.clone(), grid);
                        }
                        BlockRoute::Prefix => {
                            // Each trigger individually activates this layer.
                            for &k in &layer_ks {
                                layout
                                    .prefix_maps
                                    .entry(vec![k])
                                    .or_insert_with(|| grid.clone());
                                layout.prefix_triggers.insert(k);
                            }
                        }
                    }
                    for k in layer_ks {
                        layout.layer_triggers.insert(k);
                    }
                    i = end + 1;
                    continue;
                }
            }
        }

        // Plain base grid: `[...]`
        if line.starts_with('[') {
            if let Some((body, end)) = extract_block(&lines, i) {
                base_row_count = body.len();
                let grid = parse_grid(&body, encoder, InputMode::Direct, 0, keyboard)?;
                layout.single_map = grid;
                i = end + 1;
                continue;
            }
        }

        // Detect `(` paren-wrapped blocks (simultaneous in Mixed mode).
        let is_paren = line.starts_with('(');
        let effective_line = if is_paren {
            line.trim_start_matches('(')
        } else {
            line
        };

        if effective_line.starts_with('{')
            || (effective_line.starts_with('-') && !effective_line.starts_with("-option-input"))
        {
            if let Some((body, end)) = extract_block(&lines, i) {
                let names = parse_block_layer_names(effective_line);
                if names.is_empty() {
                    i += 1;
                    continue;
                }
                let mut layer_ks: Vec<KeyCode> = Vec::with_capacity(names.len());
                let mut missing: Vec<String> = vec![];
                for n in &names {
                    if let Some(kc) = layer_triggers.get(n) {
                        layer_ks.push(*kc);
                    } else if let Some(kc) = u32::from_str_radix(n, 16)
                        .ok()
                        .and_then(keycode_from_scancode)
                    {
                        layer_triggers.insert(n.clone(), kc);
                        layer_ks.push(kc);
                    } else {
                        missing.push(n.clone());
                    }
                }
                if !missing.is_empty() {
                    return Err(LoadError::UnknownTrigger(format!(
                        "layer name(s) {:?}",
                        missing
                    )));
                }
                layer_ks.sort_by_key(|k| key_sort(*k));

                let (grid_body, tap_cell) = split_tap_row(&body);
                let total_rows = base_row_count.max(grid_body.len());
                let offset = total_rows.saturating_sub(grid_body.len());
                let grid = parse_grid(grid_body, encoder, InputMode::Direct, offset, keyboard)?;

                for (n, &kc) in names.iter().zip(layer_ks.iter()) {
                    let tap_seq = match &tap_cell {
                        Some(cell) if is_self_marker(cell, &names) => {
                            vec![OutputToken::Key {
                                code: kc,
                                mods: Modifiers::empty(),
                            }]
                        }
                        Some(cell) => compile_cell(cell, InputMode::Direct, encoder, keyboard)?,
                        None if names.len() == 1 => {
                            layout.single_map.get(&kc).cloned().unwrap_or_else(|| {
                                vec![OutputToken::Key {
                                    code: kc,
                                    mods: Modifiers::empty(),
                                }]
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
                let route = determine_route(layout.mode, is_sustained, is_paren);

                match route {
                    BlockRoute::Sustained => {
                        layout.layer_maps.insert(layer_ks.clone(), grid);
                        for &k in &layer_ks {
                            layout.sustained_triggers.insert(k);
                        }
                    }
                    BlockRoute::Combo => {
                        for (&content, out) in &grid {
                            let mut chord = layer_ks.clone();
                            chord.push(content);
                            rmap_core::layout::canon_sort(&mut chord);
                            chord.dedup();
                            layout.combos.entry(chord).or_insert_with(|| out.clone());
                            layout.combo_keys.insert(content);
                        }
                        for &k in &layer_ks {
                            layout.combo_keys.insert(k);
                        }
                        layout.layer_maps.insert(layer_ks.clone(), grid);
                    }
                    BlockRoute::Prefix => {
                        layout.prefix_maps.insert(layer_ks.clone(), grid);
                        for &k in &layer_ks {
                            layout.prefix_triggers.insert(k);
                        }
                    }
                }
                for k in layer_ks {
                    layout.layer_triggers.insert(k);
                }
                i = end + 1;
                continue;
            }
        }

        i += 1;
    }

    Ok(layout)
}

enum BlockRoute {
    Sustained,
    Combo,
    Prefix,
}

fn determine_route(mode: LayoutMode, is_sustained: bool, is_paren: bool) -> BlockRoute {
    if is_sustained {
        return BlockRoute::Sustained;
    }
    match mode {
        LayoutMode::Legacy | LayoutMode::Simultaneous => BlockRoute::Combo,
        LayoutMode::Sequential => BlockRoute::Prefix,
        LayoutMode::Mixed => {
            if is_paren {
                BlockRoute::Combo
            } else {
                BlockRoute::Prefix
            }
        }
    }
}
