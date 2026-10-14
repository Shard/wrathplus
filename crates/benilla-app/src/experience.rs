//! Client experience, independent of the installed data and wire dialect.
//!
//! Classic is the eval default. Plus features are queried here, not through environment checks
//! in individual systems. Profile/feature setters also provide the runtime switch seam.

use std::collections::BTreeMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    #[default]
    Classic,
    Plus,
}

/// The closed, typed feature registry. Add a feature here before any consumer opts into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    AssetOverrides,
    Shadows,
    Fog,
    Bloom,
    ToneMapping,
    ColourGrading,
}

impl Feature {
    pub const ALL: [Self; 6] = [
        Self::AssetOverrides,
        Self::Shadows,
        Self::Fog,
        Self::Bloom,
        Self::ToneMapping,
        Self::ColourGrading,
    ];

    pub const fn plus_default(self) -> bool {
        true
    }
}

/// `[experience]` in the same config.toml as `[cvars]`. Pack order is ascending priority.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ExperienceConfig {
    pub profile: Profile,
    pub features: BTreeMap<Feature, bool>,
    pub packs: Vec<String>,
    pub allow_classic_overrides: bool,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct Experience {
    config: ExperienceConfig,
}

impl Experience {
    fn resolve(mut config: ExperienceConfig, env: Option<&str>) -> Result<Self, String> {
        if let Some(env) = env {
            config.profile = match env {
                "classic" => Profile::Classic,
                "plus" => Profile::Plus,
                _ => {
                    return Err(format!(
                        "invalid WOW_EXPERIENCE={env:?}; expected classic|plus"
                    ))
                }
            };
        }
        Ok(Self { config })
    }

    pub fn profile(&self) -> Profile {
        self.config.profile
    }

    pub fn set_profile(&mut self, profile: Profile) {
        self.config.profile = profile;
    }

    pub fn set_feature(&mut self, feature: Feature, enabled: bool) {
        self.config.features.insert(feature, enabled);
    }

    pub fn enabled(&self, feature: Feature) -> bool {
        if self.profile() == Profile::Classic {
            // Only this explicit opt-in can bypass the Classic mask; graphics never can.
            return feature == Feature::AssetOverrides
                && self.config.allow_classic_overrides
                && self.config.features.get(&feature).copied().unwrap_or(true);
        }
        self.config
            .features
            .get(&feature)
            .copied()
            .unwrap_or(feature.plus_default())
    }

    /// Only boot resolves packs; ordinary archive opens and test harnesses never enable them.
    pub(crate) fn asset_overrides(&self) -> anyhow::Result<benilla_formats::AssetOverrides> {
        if !self.enabled(Feature::AssetOverrides) || self.packs().is_empty() {
            return Ok(Default::default());
        }
        let home = crate::local_state::home()
            .ok_or_else(|| anyhow::anyhow!("packs require a local state home"))?;
        let mut roots = Vec::new();
        for pack in self.packs() {
            anyhow::ensure!(
                !pack.is_empty() && pack != "." && pack != ".." && !pack.contains(['/', '\\', ':']),
                "invalid pack name {pack:?}"
            );
            roots.push(home.join("mods").join(pack));
        }
        benilla_formats::AssetOverrides::from_roots(roots)
    }

    pub fn packs(&self) -> &[String] {
        &self.config.packs
    }
}

/// Resolve before asset sources/plugins build. Captures skip persistent config via config_path.
pub(crate) fn boot() -> Experience {
    let config = crate::local_state::config_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| config_from_text(&text))
        .transpose()
        .unwrap_or_else(|error| {
            eprintln!("experience: {error} — running on defaults");
            None
        })
        .unwrap_or_default();
    Experience::resolve(config, std::env::var("WOW_EXPERIENCE").ok().as_deref())
        .expect("valid explicit WOW_EXPERIENCE configuration")
}

fn config_from_text(text: &str) -> Result<ExperienceConfig, toml::de::Error> {
    #[derive(Deserialize)]
    struct Config {
        #[serde(default)]
        experience: ExperienceConfig,
    }
    toml::from_str::<Config>(text).map(|config| config.experience)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_masks_every_plus_feature_even_explicit_true() {
        let mut experience = Experience::default();
        for feature in Feature::ALL {
            assert!(!experience.enabled(feature));
            experience.set_feature(feature, true);
            assert!(!experience.enabled(feature));
        }
    }

    #[test]
    fn plus_defaults_and_runtime_switch() {
        let mut experience = Experience::default();
        experience.set_profile(Profile::Plus);
        for feature in Feature::ALL {
            assert_eq!(experience.enabled(feature), feature.plus_default());
            experience.set_feature(feature, false);
            assert!(!experience.enabled(feature));
        }
        experience.set_feature(Feature::Bloom, true);
        assert!(experience.enabled(Feature::Bloom));
        experience.set_profile(Profile::Classic);
        assert!(!experience.enabled(Feature::Bloom));
    }

    #[test]
    fn config_and_env_are_orthogonal_to_data_dialect() {
        let config = config_from_text(
            "[cvars]\nFarclip = '500'\n[experience]\nprofile = 'plus'\npacks = ['art']\n[experience.features]\nbloom = false\n",
        ).unwrap();
        let experience = Experience::resolve(config.clone(), None).unwrap();
        assert_eq!(experience.profile(), Profile::Plus);
        assert!(!experience.enabled(Feature::Bloom));
        assert_eq!(experience.packs(), &["art"]);
        let classic = Experience::resolve(config, Some("classic")).unwrap();
        assert!(Feature::ALL
            .into_iter()
            .all(|feature| !classic.enabled(feature)));
        assert!(Experience::resolve(ExperienceConfig::default(), Some("wrath")).is_err());
    }

    #[test]
    fn only_explicit_classic_asset_permission_bypasses_mask() {
        let mut config = ExperienceConfig {
            allow_classic_overrides: true,
            ..default()
        };
        let experience = Experience::resolve(config.clone(), None).unwrap();
        assert!(experience.enabled(Feature::AssetOverrides));
        for feature in Feature::ALL
            .into_iter()
            .filter(|f| *f != Feature::AssetOverrides)
        {
            assert!(!experience.enabled(feature));
        }
        config.features.insert(Feature::AssetOverrides, false);
        assert!(!Experience::resolve(config, None)
            .unwrap()
            .enabled(Feature::AssetOverrides));
    }

    #[test]
    fn unknown_features_are_rejected_and_missing_table_is_classic() {
        assert_eq!(
            config_from_text("[cvars]\n").unwrap(),
            ExperienceConfig::default()
        );
        assert!(config_from_text("[experience.features]\nboloom = true\n").is_err());
    }
}
