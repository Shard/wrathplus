//! Native build-12340 quest reward tables. Layouts: AzerothCore `DBCStructure.h`
//! `CharTitlesEntry`, `QuestXPEntry`, `TeamContributionPointsEntry`, `QuestFactionRewEntry`.
//! These readers retain
//! source values; quest XP difficulty/scaling and honor multiplier/addition belong to callers.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, i32_at, parse, str_at, u32_at};
use crate::Chain;

/// One title: the quest's title id is distinct from the player's known-title bit index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharTitle {
    pub id: u32,
    /// enUS male display template (may contain `%s`).
    pub male_name: String,
    /// enUS female display template (may contain `%s`).
    pub female_name: String,
    /// Index in `PLAYER__FIELD_KNOWN_TITLES`, not the DBC id.
    pub mask_id: u32,
}

#[derive(Debug, Default)]
pub struct CharTitlesCatalog(BTreeMap<u32, CharTitle>);

impl CharTitlesCatalog {
    pub fn get(&self, id: u32) -> Option<&CharTitle> {
        self.0.get(&id)
    }
    /// Every title, by DBC id.
    pub fn iter(&self) -> impl Iterator<Item = &CharTitle> {
        self.0.values()
    }
    /// The title at a `PLAYER__FIELD_KNOWN_TITLES` bit, as `SMSG_TITLE_EARNED` names it.
    pub fn by_bit(&self, bit_index: u32) -> Option<&CharTitle> {
        self.0.values().find(|t| t.mask_id == bit_index)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Level id to the ten native difficulty buckets. Difficulty is zero-based, never clamped.
#[derive(Debug, Default)]
pub struct QuestXpCatalog(BTreeMap<u32, [u32; 10]>);

impl QuestXpCatalog {
    pub fn get(&self, level: u32) -> Option<&[u32; 10]> {
        self.0.get(&level)
    }
    pub fn xp(&self, level: u32, difficulty: usize) -> Option<u32> {
        self.get(level)?.get(difficulty).copied()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// `QuestFactionReward.dbc`: row id (1 gains, 2 losses) to the ten signed reputation amounts a
/// quest's reward-faction index (1-9; 0 means none) selects.
#[derive(Debug, Default)]
pub struct QuestFactionRewardCatalog(BTreeMap<u32, [i32; 10]>);

impl QuestFactionRewardCatalog {
    /// A catalog of these rows, for tests.
    pub fn from_rows(rows: impl IntoIterator<Item = (u32, [i32; 10])>) -> Self {
        Self(rows.into_iter().collect())
    }
    pub fn get(&self, id: u32) -> Option<&[i32; 10]> {
        self.0.get(&id)
    }
    /// The amount for a quest's signed reward index: row 2 when negative, else row 1, at its
    /// absolute value (AzerothCore `Player::RewardReputation`).
    pub fn amount(&self, index: i32) -> Option<i32> {
        let row = if index >= 0 { 1 } else { 2 };
        self.get(row)?.get(index.unsigned_abs() as usize).copied()
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The explicit id column indexes honor values; it is not a row ordinal.
#[derive(Debug, Default)]
pub struct TeamContributionPointsCatalog(BTreeMap<u32, f32>);

impl TeamContributionPointsCatalog {
    pub fn get(&self, level: u32) -> Option<f32> {
        self.0.get(&level).copied()
    }
    /// Clamp to the highest table id, as `Quest::CalculateHonorGain` clamps to GT_MAX_LEVEL.
    /// Unknown lower ids remain unavailable rather than substituting another row.
    pub fn get_clamped(&self, level: u32) -> Option<f32> {
        self.get(level.min(*self.0.last_key_value()?.0))
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn schema(name: &str, fields: usize, strings: &[usize], floats: &[usize]) -> Schema {
    let mut s = Schema::new(name);
    for i in 0..fields {
        let ty = if strings.contains(&i) {
            FieldType::String
        } else if floats.contains(&i) {
            FieldType::Float32
        } else {
            FieldType::UInt32
        };
        s.add_field(SchemaField::new(format!("c{i}"), ty));
    }
    s
}

fn titles(bytes: &[u8]) -> Result<CharTitlesCatalog> {
    // 16 locale offsets + flag for each gender, after id/condition.
    let rs = parse(bytes, schema("CharTitles", 37, &[2, 19], &[]), "CharTitles")?;
    let mut rows = BTreeMap::new();
    for r in rs.records() {
        let id = u32_at(r, 0).context("CharTitles id")?;
        rows.insert(
            id,
            CharTitle {
                id,
                male_name: str_at(&rs, r, 2).unwrap_or_default(),
                female_name: str_at(&rs, r, 19).unwrap_or_default(),
                mask_id: u32_at(r, 36).context("CharTitles mask")?,
            },
        );
    }
    Ok(CharTitlesCatalog(rows))
}

fn quest_xp(bytes: &[u8]) -> Result<QuestXpCatalog> {
    let rs = parse(bytes, schema("QuestXP", 11, &[], &[]), "QuestXP")?;
    let mut rows = BTreeMap::new();
    for r in rs.records() {
        let id = u32_at(r, 0).context("QuestXP level")?;
        let mut xp = [0; 10];
        for (i, value) in xp.iter_mut().enumerate() {
            *value = u32_at(r, i + 1).context("QuestXP bucket")?;
        }
        rows.insert(id, xp);
    }
    Ok(QuestXpCatalog(rows))
}

fn quest_faction_reward(bytes: &[u8]) -> Result<QuestFactionRewardCatalog> {
    let rs = parse(
        bytes,
        schema("QuestFactionReward", 11, &[], &[]),
        "QuestFactionReward",
    )?;
    let mut rows = BTreeMap::new();
    for r in rs.records() {
        let id = u32_at(r, 0).context("QuestFactionReward id")?;
        let mut values = [0; 10];
        for (i, value) in values.iter_mut().enumerate() {
            *value = i32_at(r, i + 1).context("QuestFactionReward value")?;
        }
        rows.insert(id, values);
    }
    Ok(QuestFactionRewardCatalog(rows))
}

fn team_points(bytes: &[u8]) -> Result<TeamContributionPointsCatalog> {
    let rs = parse(
        bytes,
        schema("TeamContributionPoints", 2, &[], &[1]),
        "TeamContributionPoints",
    )?;
    let mut rows = BTreeMap::new();
    for r in rs.records() {
        rows.insert(
            u32_at(r, 0).context("TeamContributionPoints level")?,
            f32_at(r, 1).context("TeamContributionPoints value")?,
        );
    }
    Ok(TeamContributionPointsCatalog(rows))
}

pub fn load_char_titles(chain: &mut Chain) -> Result<CharTitlesCatalog> {
    titles(
        &chain
            .read_file("DBFilesClient\\CharTitles.dbc")
            .context("reading CharTitles")?,
    )
}
pub fn load_quest_xp(chain: &mut Chain) -> Result<QuestXpCatalog> {
    quest_xp(
        &chain
            .read_file("DBFilesClient\\QuestXP.dbc")
            .context("reading QuestXP")?,
    )
}
pub fn load_quest_faction_reward(chain: &mut Chain) -> Result<QuestFactionRewardCatalog> {
    quest_faction_reward(
        &chain
            .read_file("DBFilesClient\\QuestFactionReward.dbc")
            .context("reading QuestFactionReward")?,
    )
}
pub fn load_team_contribution_points(chain: &mut Chain) -> Result<TeamContributionPointsCatalog> {
    team_points(
        &chain
            .read_file("DBFilesClient\\TeamContributionPoints.dbc")
            .context("reading TeamContributionPoints")?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dbc(rows: &[Vec<u32>], strings: &[u8]) -> Vec<u8> {
        let fields = rows[0].len() as u32;
        let mut b = b"WDBC".to_vec();
        for n in [rows.len() as u32, fields, fields * 4, strings.len() as u32] {
            b.extend_from_slice(&n.to_le_bytes());
        }
        for row in rows {
            for v in row {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b.extend_from_slice(strings);
        b
    }

    #[test]
    fn title_id_gender_templates_and_mask_are_distinct() {
        let mut r = vec![0; 37];
        r[0] = 123;
        r[1] = 999;
        r[2] = 1;
        r[19] = 9;
        r[36] = 7;
        let cat = titles(&dbc(&[r], b"\0Sir %s\0\0Dame %s\0")).unwrap();
        let title = cat.get(123).unwrap();
        assert_eq!(
            (
                title.male_name.as_str(),
                title.female_name.as_str(),
                title.mask_id
            ),
            ("Sir %s", "Dame %s", 7)
        );
        assert!(cat.get(7).is_none());
    }

    #[test]
    fn xp_preserves_ten_zero_based_buckets_and_sparse_level_ids() {
        let cat = quest_xp(&dbc(
            &[vec![80, 0, 10, 20, 30, 40, 50, 60, 70, 80, 90]],
            b"\0",
        ))
        .unwrap();
        assert_eq!(cat.xp(80, 0), Some(0));
        assert_eq!(cat.xp(80, 9), Some(90));
        assert_eq!(cat.xp(80, 10), None);
        assert_eq!(cat.xp(1, 0), None);
    }

    #[test]
    fn honor_uses_explicit_ids_float_values_and_upper_clamp() {
        let cat = team_points(&dbc(
            &[vec![1, 1.25f32.to_bits()], vec![100, 99.5f32.to_bits()]],
            b"\0",
        ))
        .unwrap();
        assert_eq!(cat.get(1), Some(1.25));
        assert_eq!(cat.get(0), None);
        assert_eq!(cat.get_clamped(101), Some(99.5));
        assert_eq!(cat.get_clamped(2), None);
    }

    #[test]
    fn readers_reject_wrong_layouts_and_truncated_records() {
        let b = dbc(&[vec![1, 2]], b"\0");
        assert!(titles(&b).is_err());
        assert!(quest_xp(&b).is_err());
        assert!(team_points(&b[..b.len() - 2]).is_err());
    }

    #[test]
    fn installed_wrath_reward_rows() {
        if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
            return;
        }
        let data = std::env::var("WOW_DATA").expect("WOW_DATA");
        let mut chain = Chain::open_wrath(std::path::Path::new(&data), "enUS").unwrap();
        let titles = load_char_titles(&mut chain).unwrap();
        let xp = load_quest_xp(&mut chain).unwrap();
        let honor = load_team_contribution_points(&mut chain).unwrap();
        eprintln!(
            "title1={:?}, xp1={:?}, xp80={:?}, honor1={:?}, honor80={:?}",
            titles.get(1),
            xp.get(1),
            xp.get(80),
            honor.get(1),
            honor.get(80)
        );
        assert_eq!(titles.get(1).unwrap().male_name, "Private %s");
        assert_eq!(titles.get(1).unwrap().mask_id, 1);
        assert_eq!(titles.get(1).unwrap().female_name, "Private %s");
        assert_eq!(xp.get(1), Some(&[0, 10, 20, 40, 60, 80, 100, 120, 160, 0]));
        assert_eq!(
            xp.get(80),
            Some(&[0, 2200, 5500, 11050, 16550, 22050, 27550, 33100, 44100, 0])
        );
        assert_eq!(honor.get(1), Some(28.0));
        assert!((honor.get(80).unwrap() - 1241.1431).abs() < 0.0001);
    }
}
