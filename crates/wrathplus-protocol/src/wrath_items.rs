//! Native item-query codecs for Wrath 3.3.5a, build 12340.
//!
//! The positive response layout follows AzerothCore `ItemHandler.cpp:389-547` and
//! `ItemTemplate.h`. It intentionally does not call the Vanilla item parser: Wrath inserts a
//! sound-override subclass, a second flags word, a counted stat run, scaling-stat fields, and a
//! different two-entry damage run, then extends the tail through `HolidayId`.

use std::fmt;

use crate::types::{ItemDamage, ItemInfo, ItemSpellEntry, ItemUseSpell};

pub const MAX_ITEM_STATS: u32 = 10;
const ITEM_DAMAGE_BLOCKS: usize = 2;
const ITEM_SPELL_BLOCKS: usize = 5;
pub const ITEM_SOCKET_BLOCKS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemCodecError {
    Truncated,
    TrailingBytes(usize),
    StatCountOutOfRange(u32),
}

impl fmt::Display for ItemCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(f, "truncated Wrath item-query response"),
            Self::TrailingBytes(count) => {
                write!(f, "{count} trailing byte(s) in Wrath item-query response")
            }
            Self::StatCountOutOfRange(count) => write!(
                f,
                "Wrath item-query stat count {count} exceeds {MAX_ITEM_STATS}"
            ),
        }
    }
}

impl std::error::Error for ItemCodecError {}

pub type Result<T> = std::result::Result<T, ItemCodecError>;

/// Encode `CMSG_ITEM_QUERY_SINGLE`: an unpacked entry followed by an unpacked item GUID.
///
/// AzerothCore consumes only the entry, but build 12340's client body includes the full GUID.
pub fn encode_item_query(entry: u32, guid: u64) -> Vec<u8> {
    let mut body = Vec::with_capacity(12);
    body.extend(entry.to_le_bytes());
    body.extend(guid.to_le_bytes());
    body
}

/// Native template metadata used by build-12340 loot-roll reason computation (`0x6fa7a0`).
/// Keep this separate from the expansion-neutral `ItemInfo` projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollMetadata {
    pub flags2: u32,
    /// Signed template +0x1e0: negative means this item cannot be disenchanted.
    pub required_disenchant_skill: i32,
    pub item_limit_category: u32,
}

/// The template's sockets, for the socketing window: per socket block its colour mask (0 none;
/// AzerothCore `SocketColor`) and raw content word, the bonus enchant, and the gem's own
/// `GemProperties` id (0 for an item that is no gem).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SocketMetadata {
    pub colors: [u32; ITEM_SOCKET_BLOCKS],
    pub raw_contents: [u32; ITEM_SOCKET_BLOCKS],
    pub bonus: u32,
    pub gem_properties: u32,
}

/// A lossless roll-metadata seam alongside the existing item projection.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeItemTemplate {
    pub info: ItemInfo,
    pub roll: RollMetadata,
    pub sockets: SocketMetadata,
    /// `[ScalingStatDistribution id, ScalingStatValue mask]`, an heirloom's scaling (`0x61f550`).
    pub scaling: [u32; 2],
}

/// Decode the expansion-neutral projection without changing legacy callers.
pub fn decode_item_query_response(body: &[u8]) -> Result<(u32, Option<ItemInfo>)> {
    let (entry, template) = decode_native_item_query_response(body)?;
    Ok((entry, template.map(|template| template.info)))
}

/// Decode the complete native response, retaining fields needed by `0x6fa7a0` and ownership
/// helper `0x754dd0`. Negative lookup returns `None`; truncation and trailing bytes are rejected.
/// The wire layout follows AzerothCore `ItemHandler.cpp::HandleItemQuerySingleOpcode`.
pub fn decode_native_item_query_response(body: &[u8]) -> Result<(u32, Option<NativeItemTemplate>)> {
    let mut input = Input::new(body);
    let wire_entry = input.u32()?;
    if wire_entry & 0x8000_0000 != 0 {
        input.finish()?;
        return Ok((wire_entry & 0x7fff_ffff, None));
    }

    let class = input.u32()?;
    let subclass = input.u32()?;
    let _sound_override_subclass = input.i32()?;
    let name = input.cstring()?;
    for _ in 0..3 {
        let _alternate_name = input.cstring()?;
    }
    let display_info_id = input.u32()?;
    let quality = input.u32()?;
    let flags = input.u32()?;
    let flags2 = input.u32()?;
    let buy_price = input.u32()?;
    let sell_price = input.u32()?;
    let inventory_type = input.u32()?;
    let allowable_class = input.i32()?;
    let allowable_race = input.i32()?;
    let item_level = input.u32()?;
    let required_level = input.u32()?;
    let required_skill = input.u32()?;
    let required_skill_rank = input.u32()?;
    let required_spell = input.u32()?;
    let required_honor_rank = input.u32()?;
    let required_city_rank = input.u32()?;
    let required_rep_faction = input.u32()?;
    let required_rep_rank = input.u32()?;
    let max_count = input.u32()?;
    let stackable = input.u32()?;
    let container_slots = input.u32()?;

    let stat_count = input.u32()?;
    if stat_count > MAX_ITEM_STATS {
        return Err(ItemCodecError::StatCountOutOfRange(stat_count));
    }
    let mut stats = Vec::with_capacity(stat_count as usize);
    for _ in 0..stat_count {
        stats.push((input.u32()?, input.i32()?));
    }
    let scaling = [input.u32()?, input.u32()?];

    let mut damages = Vec::with_capacity(ITEM_DAMAGE_BLOCKS);
    let mut dmg_min = 0.0;
    let mut dmg_max = 0.0;
    let mut dmg_type = 0;
    for block in 0..ITEM_DAMAGE_BLOCKS {
        let min = input.f32()?;
        let max = input.f32()?;
        let school = input.u32()?;
        if block == 0 {
            dmg_min = min;
            dmg_max = max;
            dmg_type = school;
        }
        if max > 0.0 {
            damages.push(ItemDamage { min, max, school });
        }
    }

    let armor = input.u32()?;
    let resistances = [
        input.i32()?,
        input.i32()?,
        input.i32()?,
        input.i32()?,
        input.i32()?,
        input.i32()?,
    ];
    let delay_ms = input.u32()?;
    let ammo_type = input.u32()?;
    let ranged_mod_range = input.f32()?;

    let mut spells = Vec::with_capacity(ITEM_SPELL_BLOCKS);
    let mut spell_charges_0 = 0;
    let mut use_spell = None;
    for index in 0..ITEM_SPELL_BLOCKS as u8 {
        let spell_id = input.u32()?;
        let trigger = input.u32()?;
        let charges = input.i32()?;
        let cooldown_ms = input.i32()?;
        let category = input.u32()?;
        let category_cooldown_ms = input.i32()?;
        if index == 0 {
            spell_charges_0 = charges;
        }
        if spell_id != 0 {
            spells.push(ItemSpellEntry {
                index,
                spell_id,
                trigger,
                charges,
                cooldown_ms,
                category,
                category_cooldown_ms,
            });
            if use_spell.is_none() && trigger == 0 {
                use_spell = Some(ItemUseSpell {
                    spell_id,
                    cooldown_ms,
                    category,
                    category_cooldown_ms,
                });
            }
        }
    }

    let bonding = input.u32()?;
    let description = input.cstring()?;
    let page_text = input.u32()?;
    let language_id = input.u32()?;
    let page_material = input.u32()?;
    let start_quest = input.u32()?;
    let lock_id = input.u32()?;
    let material = input.u32()?;
    let sheath = input.u32()?;
    let random_property = input.u32()?;
    let _random_suffix = input.i32()?;
    let block = input.u32()?;
    let item_set = input.u32()?;
    let max_durability = input.u32()?;
    let area = input.u32()?;
    let map = input.u32()?;
    let bag_family = input.u32()?;
    let _totem_category = input.u32()?;
    let mut sockets = SocketMetadata::default();
    for i in 0..ITEM_SOCKET_BLOCKS {
        sockets.colors[i] = input.u32()?;
        sockets.raw_contents[i] = input.u32()?;
    }
    sockets.bonus = input.u32()?;
    sockets.gem_properties = input.u32()?;
    let required_disenchant_skill = input.i32()?;
    let _armor_damage_modifier = input.f32()?;
    let _duration = input.u32()?;
    let item_limit_category = input.u32()?;
    let _holiday_id = input.u32()?;
    input.finish()?;

    Ok((
        wire_entry,
        Some(NativeItemTemplate {
            sockets,
            scaling,
            roll: RollMetadata {
                flags2,
                required_disenchant_skill,
                item_limit_category,
            },
            info: ItemInfo {
                class,
                subclass,
                name,
                display_info_id,
                quality,
                flags,
                buy_price,
                sell_price,
                inventory_type,
                allowable_class,
                allowable_race,
                item_level,
                required_level,
                required_skill,
                required_skill_rank,
                required_spell,
                required_honor_rank,
                required_city_rank,
                required_rep_faction,
                required_rep_rank,
                max_count,
                stackable,
                container_slots,
                stats,
                damages,
                dmg_min,
                dmg_max,
                dmg_type,
                armor,
                resistances,
                delay_ms,
                ammo_type,
                ranged_mod_range,
                spells,
                spell_charges_0,
                use_spell,
                bonding,
                description,
                page_text,
                language_id,
                page_material,
                start_quest,
                lock_id,
                material,
                sheath,
                random_property,
                block,
                item_set,
                max_durability,
                area,
                map,
                bag_family,
            },
        }),
    ))
}

struct Input<'a> {
    remaining: &'a [u8],
}

impl<'a> Input<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        if self.remaining.len() < count {
            return Err(ItemCodecError::Truncated);
        }
        let (value, remaining) = self.remaining.split_at(count);
        self.remaining = remaining;
        Ok(value)
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four-byte slice"),
        ))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn cstring(&mut self) -> Result<String> {
        let end = self
            .remaining
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(ItemCodecError::Truncated)?;
        let bytes = self.take(end + 1)?;
        Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }

    fn finish(self) -> Result<()> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(ItemCodecError::TrailingBytes(self.remaining.len()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(out: &mut Vec<u8>, value: u32) {
        out.extend(value.to_le_bytes());
    }

    fn signed(out: &mut Vec<u8>, value: i32) {
        word(out, value as u32);
    }

    fn float(out: &mut Vec<u8>, value: f32) {
        word(out, value.to_bits());
    }

    fn text(out: &mut Vec<u8>, value: &str) {
        out.extend(value.as_bytes());
        out.push(0);
    }

    fn minimal_positive() -> Vec<u8> {
        let mut out = Vec::new();
        word(&mut out, 1); // entry
        word(&mut out, 0); // class
        word(&mut out, 0); // subclass
        signed(&mut out, 0); // sound override
        for _ in 0..4 {
            text(&mut out, "");
        }
        for _ in 0..24 {
            word(&mut out, 0); // display through StatsCount, ScalingStatDistribution and Value
        }
        for _ in 0..ITEM_DAMAGE_BLOCKS {
            float(&mut out, 0.0);
            float(&mut out, 0.0);
            word(&mut out, 0);
        }
        for _ in 0..10 {
            word(&mut out, 0); // seven resistances, delay, ammo, range
        }
        for _ in 0..ITEM_SPELL_BLOCKS {
            for value in [0, 0, 0, u32::MAX, 0, u32::MAX] {
                word(&mut out, value);
            }
        }
        word(&mut out, 0); // bonding
        text(&mut out, "");
        for _ in 0..16 {
            word(&mut out, 0); // PageText through TotemCategory
        }
        for _ in 0..ITEM_SOCKET_BLOCKS * 2 {
            word(&mut out, 0);
        }
        for _ in 0..7 {
            word(&mut out, 0); // socketBonus through HolidayId
        }
        out
    }

    fn dense_positive() -> Vec<u8> {
        let mut out = Vec::new();
        word(&mut out, 0x0102_0304);
        word(&mut out, 2);
        word(&mut out, 7);
        signed(&mut out, -8);
        text(&mut out, "Frostbrand");
        text(&mut out, "name two");
        text(&mut out, "name three");
        text(&mut out, "name four");
        for value in [101, 4, 0x8000_0008, 0x1020_3040, 123_456, 654_321, 21] {
            word(&mut out, value);
        }
        signed(&mut out, -1);
        signed(&mut out, i32::MIN);
        for value in [245, 80, 164, 375, 9001, 14, 17, 72, 8] {
            word(&mut out, value);
        }
        signed(&mut out, -2);
        signed(&mut out, -20);
        word(&mut out, 36);
        word(&mut out, 3);
        for (kind, value) in [(3, 91), (32, -7), (48, 1234)] {
            word(&mut out, kind);
            signed(&mut out, value);
        }
        word(&mut out, 55);
        word(&mut out, 0xa5a5_5a5a);
        float(&mut out, 12.25);
        float(&mut out, 34.5);
        word(&mut out, 4);
        float(&mut out, -1.5);
        float(&mut out, 2.75);
        word(&mut out, 6);
        word(&mut out, 700);
        for value in [-1, 2, -3, 4, -5, 6] {
            signed(&mut out, value);
        }
        word(&mut out, 2800);
        word(&mut out, 3);
        float(&mut out, 1.75);
        for (id, trigger, charges, cooldown, category, category_cooldown) in [
            (111, 1, 5, 6000, 12, 7000),
            (222, 0, -3, -1, 13, -1),
            (333, 2, 0, 8000, 14, 9000),
            (0, 0, 0, -1, 0, -1),
            (444, 5, 1, 10_000, 15, 11_000),
        ] {
            word(&mut out, id);
            word(&mut out, trigger);
            signed(&mut out, charges);
            signed(&mut out, cooldown);
            word(&mut out, category);
            signed(&mut out, category_cooldown);
        }
        word(&mut out, 2);
        text(&mut out, "Cold enough to burn.");
        for value in [501, 7, 9, 601, 701] {
            word(&mut out, value);
        }
        signed(&mut out, -11);
        word(&mut out, 3);
        signed(&mut out, -801);
        signed(&mut out, -802);
        for value in [901, 1001, 1101, 1201, 1301, 0x0000_4121, 1401] {
            word(&mut out, value);
        }
        for (color, content) in [(1, 1501), (2, 1502), (8, 1503)] {
            word(&mut out, color);
            word(&mut out, content);
        }
        word(&mut out, 1601);
        word(&mut out, 1701);
        word(&mut out, 450);
        float(&mut out, -0.125);
        word(&mut out, 1801);
        word(&mut out, 1901);
        word(&mut out, 2001);
        out
    }

    #[test]
    fn native_roll_metadata_retains_flags_category_and_signed_skill() {
        let mut body = dense_positive();
        let skill_offset = body.len() - 20;
        for skill in [i32::MIN, -1, 0, 450, i32::MAX] {
            body[skill_offset..skill_offset + 4].copy_from_slice(&skill.to_le_bytes());
            let (entry, template) = decode_native_item_query_response(&body).unwrap();
            let template = template.unwrap();
            assert_eq!(entry, 0x0102_0304);
            assert_eq!(template.scaling, [55, 0xa5a5_5a5a], "after the stat run");
            assert_eq!(
                template.roll,
                RollMetadata {
                    flags2: 0x1020_3040,
                    required_disenchant_skill: skill,
                    item_limit_category: 1901,
                }
            );
            assert_eq!(
                decode_item_query_response(&body).unwrap(),
                (entry, Some(template.info))
            );
        }
        assert_eq!(
            decode_native_item_query_response(&0x8000_0123u32.to_le_bytes()).unwrap(),
            (0x123, None)
        );
        for end in 0..body.len() {
            assert!(decode_native_item_query_response(&body[..end]).is_err());
        }
        body.push(0);
        assert!(matches!(
            decode_native_item_query_response(&body),
            Err(ItemCodecError::TrailingBytes(1))
        ));
    }

    #[test]
    fn query_body_is_native_entry_and_full_guid() {
        assert_eq!(
            encode_item_query(0x1234_5678, 0x0123_4567_89ab_cdef),
            [0x78, 0x56, 0x34, 0x12, 0xef, 0xcd, 0xab, 0x89, 0x67, 0x45, 0x23, 0x01,]
        );
    }

    #[test]
    fn dense_positive_golden_projects_shared_fields_and_consumes_wrath_tail() {
        let (entry, item) = decode_item_query_response(&dense_positive()).unwrap();
        let item = item.unwrap();
        assert_eq!(entry, 0x0102_0304);
        assert_eq!((item.class, item.subclass), (2, 7));
        assert_eq!(item.name, "Frostbrand");
        assert_eq!(item.display_info_id, 101);
        assert_eq!((item.quality, item.flags), (4, 0x8000_0008));
        assert_eq!((item.buy_price, item.sell_price), (123_456, 654_321));
        assert_eq!(item.inventory_type, 21);
        assert_eq!((item.allowable_class, item.allowable_race), (-1, i32::MIN));
        assert_eq!(
            (
                item.item_level,
                item.required_level,
                item.required_skill,
                item.required_skill_rank,
                item.required_spell,
                item.required_honor_rank,
                item.required_city_rank,
                item.required_rep_faction,
                item.required_rep_rank,
            ),
            (245, 80, 164, 375, 9001, 14, 17, 72, 8)
        );
        assert_eq!(
            (item.max_count, item.stackable, item.container_slots),
            (u32::MAX - 1, u32::MAX - 19, 36)
        );
        assert_eq!(item.stats, [(3, 91), (32, -7), (48, 1234)]);
        assert_eq!(item.damages.len(), 2);
        assert_eq!(
            item.damages[0],
            ItemDamage {
                min: 12.25,
                max: 34.5,
                school: 4
            }
        );
        assert_eq!(
            item.damages[1],
            ItemDamage {
                min: -1.5,
                max: 2.75,
                school: 6
            }
        );
        assert_eq!(
            (item.dmg_min, item.dmg_max, item.dmg_type),
            (12.25, 34.5, 4)
        );
        assert_eq!(item.armor, 700);
        assert_eq!(item.resistances, [-1, 2, -3, 4, -5, 6]);
        assert_eq!(
            (item.delay_ms, item.ammo_type, item.ranged_mod_range),
            (2800, 3, 1.75)
        );
        assert_eq!(item.spells.len(), 4);
        assert_eq!(item.spells[1].index, 1);
        assert_eq!(item.spells[1].spell_id, 222);
        assert_eq!(item.spell_charges_0, 5);
        assert_eq!(
            item.use_spell,
            Some(ItemUseSpell {
                spell_id: 222,
                cooldown_ms: -1,
                category: 13,
                category_cooldown_ms: -1
            })
        );
        assert_eq!(item.bonding, 2);
        assert_eq!(item.description, "Cold enough to burn.");
        assert_eq!(
            (item.page_text, item.language_id, item.page_material),
            (501, 7, 9)
        );
        assert_eq!(
            (item.start_quest, item.lock_id, item.material, item.sheath),
            (601, 701, u32::MAX - 10, 3)
        );
        assert_eq!(item.random_property, u32::MAX - 800);
        assert_eq!(
            (item.block, item.item_set, item.max_durability),
            (901, 1001, 1101)
        );
        assert_eq!(
            (item.area, item.map, item.bag_family),
            (1201, 1301, 0x0000_4121)
        );
    }

    #[test]
    fn minimum_positive_response_is_accepted() {
        let (entry, item) = decode_item_query_response(&minimal_positive()).unwrap();
        let item = item.unwrap();
        assert_eq!(entry, 1);
        assert!(item.name.is_empty());
        assert!(item.stats.is_empty());
        assert!(item.damages.is_empty());
        assert!(item.spells.is_empty());
        assert_eq!(item.spell_charges_0, 0);
    }

    #[test]
    fn negative_response_clears_high_bit_without_fabricating_item() {
        assert_eq!(
            decode_item_query_response(&0xf234_5678u32.to_le_bytes()).unwrap(),
            (0x7234_5678, None)
        );
    }

    #[test]
    fn every_positive_prefix_is_truncated() {
        let packet = dense_positive();
        for end in 0..packet.len() {
            assert_eq!(
                decode_item_query_response(&packet[..end]),
                Err(ItemCodecError::Truncated),
                "prefix length {end}"
            );
        }
    }

    #[test]
    fn every_negative_prefix_is_truncated() {
        let packet = 0x8000_0042u32.to_le_bytes();
        for end in 0..packet.len() {
            assert_eq!(
                decode_item_query_response(&packet[..end]),
                Err(ItemCodecError::Truncated),
                "prefix length {end}"
            );
        }
    }

    #[test]
    fn positive_and_negative_responses_reject_trailing_bytes() {
        let mut positive = minimal_positive();
        positive.extend([1, 2, 3]);
        assert_eq!(
            decode_item_query_response(&positive),
            Err(ItemCodecError::TrailingBytes(3))
        );
        let mut negative = 0x8000_0042u32.to_le_bytes().to_vec();
        negative.push(9);
        assert_eq!(
            decode_item_query_response(&negative),
            Err(ItemCodecError::TrailingBytes(1))
        );
    }

    #[test]
    fn stat_count_accepts_protocol_maximum_and_rejects_eleven() {
        let mut maximum = minimal_positive();
        // Entry, three fixed words, four empty C strings, then 21 fixed words precede StatsCount.
        let stat_count_offset = 4 + 12 + 4 + 21 * 4;
        maximum[stat_count_offset..stat_count_offset + 4]
            .copy_from_slice(&MAX_ITEM_STATS.to_le_bytes());
        maximum.splice(stat_count_offset + 4..stat_count_offset + 4, [0; 80]);
        assert_eq!(
            decode_item_query_response(&maximum)
                .unwrap()
                .1
                .unwrap()
                .stats
                .len(),
            MAX_ITEM_STATS as usize
        );

        let mut excessive = minimal_positive();
        excessive[stat_count_offset..stat_count_offset + 4]
            .copy_from_slice(&(MAX_ITEM_STATS + 1).to_le_bytes());
        assert_eq!(
            decode_item_query_response(&excessive),
            Err(ItemCodecError::StatCountOutOfRange(MAX_ITEM_STATS + 1))
        );
    }
}
