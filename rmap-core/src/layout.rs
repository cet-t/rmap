//! Layout: compiled mapping + metadata.

use crate::{KeyCode, OutputSeq, InputMode};
use std::collections::HashMap;

pub type LayoutId = String;

#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub id: LayoutId,
    pub name: String,
    pub input_mode: InputMode,
    /// Base (no layers): physical -> output
    pub single_map: HashMap<KeyCode, OutputSeq>,
    /// Layered shifts: sorted active layers vec -> (content key -> output)
    pub layer_maps: HashMap<Vec<KeyCode>, HashMap<KeyCode, OutputSeq>>,
    /// Tap output when a layer key is released alone (within window, no partner)
    pub layer_taps: HashMap<KeyCode, OutputSeq>,
    /// Keys that are declared as shift layers (from -option-input)
    pub layer_triggers: std::collections::HashSet<KeyCode>,
    /// Legacy simultaneous rules (kept for compatibility)
    pub simultaneous: Vec<ComboRule>,
}

#[derive(Debug, Clone)]
pub struct ComboRule {
    pub layers: Vec<KeyCode>,  // e.g. [Space] for SandS, or [Muhenkan, Henkan] etc.
    pub output: OutputSeq,
}

// LayerTap concept folded into Layout.layer_taps. This struct kept only for possible future external use.
#[derive(Debug, Clone)]
pub struct LayerTap {
    pub layer_key: KeyCode,
    pub tap_output: OutputSeq,
}

impl Layout {
    pub fn is_layer_trigger(&self, k: KeyCode) -> bool {
        self.layer_triggers.contains(&k)
    }
}
