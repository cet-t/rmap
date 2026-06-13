//! Minimal config + profile loader for prototype.
//! JSON for app->profile map and global defaults. Layouts still loaded via LayoutLoader trait (DvorakJ files).

use crate::profile::{Profile, ProfileId, AppProfileMap, ProfileToggles};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub profiles: HashMap<ProfileId, ProfileDef>,
    pub app_map: AppProfileMap,
    pub default_layout: String, // layout file path or id for the default profile
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileDef {
    pub layout: String, // path to DvorakJ .txt or later rmap-native
    #[serde(default)]
    pub toggles: ProfileToggles,
}

impl AppConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let s = std::fs::read_to_string(path)?;
        let cfg: AppConfig = serde_json::from_str(&s)?;
        Ok(cfg)
    }

    /// Construct a minimal fallback config pointing at the toy SandS sample.
    /// Used when data/config.json is missing or unreadable (NFR-4 fail-fast otherwise).
    pub fn fallback() -> Self {
        use std::collections::HashMap;
        let mut profiles = HashMap::new();
        profiles.insert(
            "default".to_string(),
            ProfileDef {
                layout: "data/layouts/samples/toy_simul.txt".to_string(),
                toggles: ProfileToggles {
                    enable_sands: true,
                    enable_gestures: false,
                    enable_shortcuts: false,
                },
            },
        );
        profiles.insert(
            "colemak".to_string(),
            ProfileDef {
                layout: "data/layouts/samples/toy_simul.txt".to_string(),
                toggles: ProfileToggles {
                    enable_sands: false,
                    enable_gestures: false,
                    enable_shortcuts: false,
                },
            },
        );
        AppConfig {
            profiles,
            app_map: AppProfileMap {
                per_app: HashMap::new(),
                default_profile: "default".to_string(),
            },
            default_layout: "data/layouts/samples/toy_simul.txt".to_string(),
        }
    }

    pub fn default_profile(&self) -> Option<Profile> {
        let id = &self.app_map.default_profile;
        self.profiles.get(id).map(|p| Profile {
            id: id.clone(),
            layout_id: p.layout.clone(),
            toggles: p.toggles.clone(),
        })
    }

    /// Resolve the layout file path for a given app_id (from per_app map or default_profile).
    /// Returns default_layout if profile missing.
    pub fn layout_path_for_app(&self, app_id: &str) -> String {
        let prof_id = self
            .app_map
            .per_app
            .get(app_id)
            .unwrap_or(&self.app_map.default_profile);
        self.profiles
            .get(prof_id)
            .map(|p| p.layout.clone())
            .unwrap_or_else(|| self.default_layout.clone())
    }
}
