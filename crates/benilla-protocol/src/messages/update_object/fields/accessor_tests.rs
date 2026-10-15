//! Distinct native descriptor values: no shared constant or zero-filled fixture can mask aliases.
use super::*;

#[test]
fn scalar_accessors_read_distinct_native_fields_in_both_dialects() {
    macro_rules! check {
        ($name:literal, $field:expr, $read:expr, $expected:expr) => {
            for layout in [FieldLayout::Vanilla5875, FieldLayout::Wrath12340] {
                let span = layout.resolve($field);
                let pairs: Vec<_> = (0..layout.descriptor_len(ObjectType::Player))
                    .map(|index| (index, 0x3f00_0000 + u32::from(index) * 257))
                    .collect();
                let fields = ObjectFields::from_native_pairs(layout, &pairs);
                let raw = span.map(|s| 0x3f00_0000 + u32::from(s.index) * 257);
                let guid = span.filter(|s| s.width >= 2).map(|s| {
                    u64::from(0x3f00_0000 + u32::from(s.index) * 257)
                        | (u64::from(0x3f00_0000 + u32::from(s.index + 1) * 257) << 32)
                });
                assert_eq!(
                    $read(&fields),
                    $expected(raw, guid),
                    "{} {:?}",
                    $name,
                    layout
                );
            }
        };
    }
    check!(
        "object_entry",
        Field::ObjectEntry,
        |fields: &ObjectFields| fields.object_entry(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "item_stack_count",
        Field::ItemStackCount,
        |fields: &ObjectFields| fields.item_stack_count(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "item_flags",
        Field::ItemFlags,
        |fields: &ObjectFields| fields.item_flags(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "item_creator",
        Field::ItemCreator,
        |fields: &ObjectFields| fields.item_creator(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            guid.filter(|&g| g != 0)
        }
    );
    check!(
        "item_text_id",
        Field::ItemItemTextId,
        |fields: &ObjectFields| fields.item_text_id(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "item_random_properties_id",
        Field::ItemRandomPropertiesId,
        |fields: &ObjectFields| fields.item_random_properties_id(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "item_durability",
        Field::ItemDurability,
        |fields: &ObjectFields| fields.item_durability(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "item_max_durability",
        Field::ItemMaxdurability,
        |fields: &ObjectFields| fields.item_max_durability(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "container_num_slots",
        Field::ContainerNumSlots,
        |fields: &ObjectFields| fields.container_num_slots(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "player_money",
        Field::PlayerCoinage,
        |fields: &ObjectFields| fields.player_money(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "player_farsight",
        Field::PlayerFarsight,
        |fields: &ObjectFields| fields.player_farsight(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            guid.filter(|&g| g != 0)
        }
    );
    check!(
        "player_xp",
        Field::PlayerXp,
        |fields: &ObjectFields| fields.player_xp(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "player_next_level_xp",
        Field::PlayerNextLevelXp,
        |fields: &ObjectFields| fields.player_next_level_xp(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "player_watched_faction",
        Field::PlayerWatchedFactionIndex,
        |fields: &ObjectFields| fields.player_watched_faction(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(|v| v as i32)
        }
    );
    check!(
        "player_talent_points",
        Field::PlayerCharacterPoints1,
        |fields: &ObjectFields| fields.player_talent_points(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "player_free_professions",
        Field::PlayerCharacterPoints2,
        |fields: &ObjectFields| fields.player_free_professions(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "player_track_creatures",
        Field::PlayerTrackCreatures,
        |fields: &ObjectFields| fields.player_track_creatures(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "player_track_resources",
        Field::PlayerTrackResources,
        |fields: &ObjectFields| fields.player_track_resources(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "player_track_stealthed",
        Field::PlayerFieldBytes,
        |fields: &ObjectFields| fields.player_track_stealthed(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0) & 0x2 != 0
        }
    );
    check!(
        "player_block_percentage",
        Field::PlayerBlockPercentage,
        |fields: &ObjectFields| fields.player_block_percentage(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(f32::from_bits)
        }
    );
    check!(
        "player_dodge_percentage",
        Field::PlayerDodgePercentage,
        |fields: &ObjectFields| fields.player_dodge_percentage(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(f32::from_bits)
        }
    );
    check!(
        "player_parry_percentage",
        Field::PlayerParryPercentage,
        |fields: &ObjectFields| fields.player_parry_percentage(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(f32::from_bits)
        }
    );
    check!(
        "player_crit_percentage",
        Field::PlayerCritPercentage,
        |fields: &ObjectFields| fields.player_crit_percentage(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(f32::from_bits)
        }
    );
    check!(
        "player_ammo_id",
        Field::PlayerAmmoId,
        |fields: &ObjectFields| fields.player_ammo_id(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "player_self_res_spell",
        Field::PlayerSelfResSpell,
        |fields: &ObjectFields| fields.player_self_res_spell(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.filter(|&s| s != 0)
        }
    );
    check!(
        "player_bytes",
        Field::PlayerBytes,
        |fields: &ObjectFields| fields.player_bytes(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "player_bytes_2",
        Field::PlayerBytes2,
        |fields: &ObjectFields| fields.player_bytes_2(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "player_rest_state_experience",
        Field::PlayerRestStateExperience,
        |fields: &ObjectFields| fields.player_rest_state_experience(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "player_drunk_byte",
        Field::PlayerBytes3,
        |fields: &ObjectFields| fields.player_drunk_byte(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(|v| (v >> 8) as u8)
        }
    );
    check!(
        "player_flags",
        Field::PlayerFlags,
        |fields: &ObjectFields| fields.player_flags(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "player_duel_arbiter",
        Field::PlayerDuelArbiter,
        |fields: &ObjectFields| fields.player_duel_arbiter(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            guid.unwrap_or(0)
        }
    );
    check!(
        "player_duel_team",
        Field::PlayerDuelTeam,
        |fields: &ObjectFields| fields.player_duel_team(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "player_guild_id",
        Field::PlayerGuildid,
        |fields: &ObjectFields| fields.player_guild_id(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "player_guild_rank",
        Field::PlayerGuildrank,
        |fields: &ObjectFields| fields.player_guild_rank(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "player_release_timer_running",
        Field::PlayerFieldBytes,
        |fields: &ObjectFields| fields.player_release_timer_running(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0) & 0x08 != 0
        }
    );
    check!(
        "player_combo_target",
        Field::PlayerComboTarget,
        |fields: &ObjectFields| fields.player_combo_target(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            guid.unwrap_or(0)
        }
    );
    check!(
        "player_action_bar_toggles",
        Field::PlayerFieldBytes,
        |fields: &ObjectFields| fields.player_action_bar_toggles(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(|b| ((b >> 16) & 0xff) as u8)
        }
    );
    check!(
        "player_honor_rank",
        Field::PlayerFieldBytes,
        |fields: &ObjectFields| fields.player_honor_rank(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(|b| ((b >> 24) & 0xff) as u8)
        }
    );
    check!(
        "player_session_kills",
        Field::PlayerSessionKills,
        |fields: &ObjectFields| fields.player_session_kills(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(|v| (v as u16, (v >> 16) as u16))
        }
    );
    check!(
        "player_yesterday_kills",
        Field::PlayerYesterdayKills,
        |fields: &ObjectFields| fields.player_yesterday_kills(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(|v| (v as u16, (v >> 16) as u16))
        }
    );
    check!(
        "player_last_week_kills",
        Field::PlayerLastWeekKills,
        |fields: &ObjectFields| fields.player_last_week_kills(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(|v| (v as u16, (v >> 16) as u16))
        }
    );
    check!(
        "player_this_week_kills",
        Field::PlayerThisWeekKills,
        |fields: &ObjectFields| fields.player_this_week_kills(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(|v| (v as u16, (v >> 16) as u16))
        }
    );
    check!(
        "player_this_week_contribution",
        Field::PlayerThisWeekContribution,
        |fields: &ObjectFields| fields.player_this_week_contribution(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "player_yesterday_contribution",
        Field::PlayerYesterdayContribution,
        |fields: &ObjectFields| fields.player_yesterday_contribution(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "player_last_week_contribution",
        Field::PlayerLastWeekContribution,
        |fields: &ObjectFields| fields.player_last_week_contribution(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "player_last_week_rank",
        Field::PlayerLastWeekRank,
        |fields: &ObjectFields| fields.player_last_week_rank(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "player_lifetime_dishonorable_kills",
        Field::PlayerLifetimeDishonorbaleKills,
        |fields: &ObjectFields| fields.player_lifetime_dishonorable_kills(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "gameobject_created_by",
        Field::ObjectCreatedBy,
        |fields: &ObjectFields| fields.gameobject_created_by(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            guid.filter(|&g| g != 0)
        }
    );
    check!(
        "gameobject_displayid",
        Field::GameobjectDisplayId,
        |fields: &ObjectFields| fields.gameobject_displayid(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(|v| v as i32)
        }
    );
    check!(
        "gameobject_flags",
        Field::GameobjectFlags,
        |fields: &ObjectFields| fields.gameobject_flags(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "gameobject_faction",
        Field::GameobjectFaction,
        |fields: &ObjectFields| fields.gameobject_faction(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "gameobject_level",
        Field::GameobjectLevel,
        |fields: &ObjectFields| fields.gameobject_level(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "is_container",
        Field::ObjectType,
        |fields: &ObjectFields| fields.is_container(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0) & 0x4 != 0
        }
    );
    check!(
        "object_scale_x",
        Field::ObjectScaleX,
        |fields: &ObjectFields| fields.object_scale_x(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(f32::from_bits)
        }
    );
    check!(
        "unit_displayid",
        Field::UnitDisplayId,
        |fields: &ObjectFields| fields.unit_displayid(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(|v| v as i32)
        }
    );
    check!(
        "unit_native_displayid",
        Field::UnitNativeDisplayId,
        |fields: &ObjectFields| fields.unit_native_displayid(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(|v| v as i32)
        }
    );
    check!(
        "unit_mount_display_id",
        Field::UnitMountDisplayId,
        |fields: &ObjectFields| fields.unit_mount_display_id(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "unit_target",
        Field::UnitTarget,
        |fields: &ObjectFields| fields.unit_target(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            guid.filter(|&g| g != 0)
        }
    );
    check!(
        "unit_channel_object",
        Field::UnitChannelObject,
        |fields: &ObjectFields| fields.unit_channel_object(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            guid.filter(|&g| g != 0)
        }
    );
    check!(
        "unit_channel_spell",
        Field::UnitChannelSpell,
        |fields: &ObjectFields| fields.unit_channel_spell(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "unit_summon",
        Field::UnitSummon,
        |fields: &ObjectFields| fields.unit_summon(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            guid.filter(|&g| g != 0)
        }
    );
    check!(
        "unit_charm",
        Field::UnitCharm,
        |fields: &ObjectFields| fields.unit_charm(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            guid.filter(|&g| g != 0)
        }
    );
    check!(
        "unit_summoned_by",
        Field::UnitSummonedby,
        |fields: &ObjectFields| fields.unit_summoned_by(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            guid.filter(|&g| g != 0)
        }
    );
    check!(
        "unit_charmed_by",
        Field::UnitCharmedby,
        |fields: &ObjectFields| fields.unit_charmed_by(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            guid.filter(|&g| g != 0)
        }
    );
    check!(
        "unit_created_by",
        Field::UnitCreatedby,
        |fields: &ObjectFields| fields.unit_created_by(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            guid.filter(|&g| g != 0)
        }
    );
    check!(
        "unit_created_by_spell",
        Field::UnitCreatedBySpell,
        |fields: &ObjectFields| fields.unit_created_by_spell(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.filter(|&s| s != 0)
        }
    );
    check!(
        "unit_pet_number",
        Field::UnitPetnumber,
        |fields: &ObjectFields| fields.unit_pet_number(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "unit_pet_name_timestamp",
        Field::UnitPetNameTimestamp,
        |fields: &ObjectFields| fields.unit_pet_name_timestamp(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "unit_health",
        Field::UnitHealth,
        |fields: &ObjectFields| fields.unit_health(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "unit_max_health",
        Field::UnitMaxHealth,
        |fields: &ObjectFields| fields.unit_max_health(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "unit_level",
        Field::UnitLevel,
        |fields: &ObjectFields| fields.unit_level(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "unit_aura_state",
        Field::UnitAuraState,
        |fields: &ObjectFields| fields.unit_aura_state(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "unit_base_mana",
        Field::UnitBaseMana,
        |fields: &ObjectFields| fields.unit_base_mana(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "unit_base_health",
        Field::UnitBaseHealth,
        |fields: &ObjectFields| fields.unit_base_health(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "unit_faction_template",
        Field::UnitFactionTemplate,
        |fields: &ObjectFields| fields.unit_faction_template(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "unit_flags",
        Field::UnitFlags,
        |fields: &ObjectFields| fields.unit_flags(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "unit_combat_reach",
        Field::UnitCombatreach,
        |fields: &ObjectFields| fields.unit_combat_reach(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(f32::from_bits).unwrap_or(1.5)
        }
    );
    check!(
        "unit_bounding_radius",
        Field::UnitBoundingradius,
        |fields: &ObjectFields| fields.unit_bounding_radius(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(f32::from_bits).unwrap_or(0.0)
        }
    );
    check!(
        "unit_dynamic_flags",
        Field::UnitDynamicFlags,
        |fields: &ObjectFields| fields.unit_dynamic_flags(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "unit_npc_flags",
        Field::UnitNpcFlags,
        |fields: &ObjectFields| fields.unit_npc_flags(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "unit_emote_state",
        Field::UnitNpcEmotestate,
        |fields: &ObjectFields| fields.unit_emote_state(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "unit_bytes_0",
        Field::UnitBytes0,
        |fields: &ObjectFields| fields.unit_bytes_0(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "unit_sheath_state",
        Field::UnitBytes2,
        |fields: &ObjectFields| fields.unit_sheath_state(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(|v| (v & 0xff) as u8)
        }
    );
    check!(
        "unit_attack_power",
        Field::UnitAttackPower,
        |fields: &ObjectFields| fields.unit_attack_power(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(|v| v as i32)
        }
    );
    check!(
        "unit_attack_power_multiplier",
        Field::UnitAttackPowerMultiplier,
        |fields: &ObjectFields| fields.unit_attack_power_multiplier(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(f32::from_bits)
        }
    );
    check!(
        "unit_ranged_attack_power",
        Field::UnitRangedAttackPower,
        |fields: &ObjectFields| fields.unit_ranged_attack_power(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(|v| v as i32)
        }
    );
    check!(
        "unit_ranged_attack_power_multiplier",
        Field::UnitRangedAttackPowerMultiplier,
        |fields: &ObjectFields| fields.unit_ranged_attack_power_multiplier(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(f32::from_bits)
        }
    );
    check!(
        "unit_min_damage",
        Field::UnitMindamage,
        |fields: &ObjectFields| fields.unit_min_damage(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(f32::from_bits)
        }
    );
    check!(
        "unit_max_damage",
        Field::UnitMaxdamage,
        |fields: &ObjectFields| fields.unit_max_damage(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(f32::from_bits)
        }
    );
    check!(
        "unit_min_offhand_damage",
        Field::UnitMinoffhanddamage,
        |fields: &ObjectFields| fields.unit_min_offhand_damage(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(f32::from_bits)
        }
    );
    check!(
        "unit_max_offhand_damage",
        Field::UnitMaxoffhanddamage,
        |fields: &ObjectFields| fields.unit_max_offhand_damage(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(f32::from_bits)
        }
    );
    check!(
        "unit_min_ranged_damage",
        Field::UnitMinrangeddamage,
        |fields: &ObjectFields| fields.unit_min_ranged_damage(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(f32::from_bits)
        }
    );
    check!(
        "unit_max_ranged_damage",
        Field::UnitMaxrangeddamage,
        |fields: &ObjectFields| fields.unit_max_ranged_damage(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(f32::from_bits)
        }
    );
    check!(
        "unit_ranged_attack_time",
        Field::UnitRangedattacktime,
        |fields: &ObjectFields| fields.unit_ranged_attack_time(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "corpse_owner",
        Field::CorpseOwner,
        |fields: &ObjectFields| fields.corpse_owner(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            guid.filter(|&g| g != 0)
        }
    );
    check!(
        "corpse_display_id",
        Field::CorpseDisplayId,
        |fields: &ObjectFields| fields.corpse_display_id(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.filter(|&d| d != 0)
        }
    );
    check!(
        "corpse_guild",
        Field::CorpseGuild,
        |fields: &ObjectFields| fields.corpse_guild(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "corpse_flags",
        Field::CorpseFlags,
        |fields: &ObjectFields| fields.corpse_flags(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0)
        }
    );
    check!(
        "corpse_flags_present",
        Field::CorpseFlags,
        |fields: &ObjectFields| fields.corpse_flags_present(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "corpse_lootable",
        Field::CorpseDynamicFlags,
        |fields: &ObjectFields| fields.corpse_lootable(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.unwrap_or(0) & 0x01 != 0
        }
    );
    check!(
        "dynamicobject_caster",
        Field::DynamicobjectCaster,
        |fields: &ObjectFields| fields.dynamicobject_caster(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            guid.filter(|&g| g != 0)
        }
    );
    check!(
        "dynamicobject_bytes",
        Field::DynamicobjectBytes,
        |fields: &ObjectFields| fields.dynamicobject_bytes(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw
        }
    );
    check!(
        "dynamicobject_spell_id",
        Field::DynamicobjectSpellid,
        |fields: &ObjectFields| fields.dynamicobject_spell_id(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.filter(|&s| s != 0)
        }
    );
    check!(
        "dynamicobject_radius",
        Field::DynamicobjectRadius,
        |fields: &ObjectFields| fields.dynamicobject_radius(),
        |raw: Option<u32>, guid: Option<u64>| {
            let _ = (raw, guid);
            raw.map(f32::from_bits)
        }
    );
}

fn fixture(layout: FieldLayout, values: &[(Field, u16, u32)]) -> ObjectFields {
    let pairs: Vec<_> = values
        .iter()
        .filter_map(|&(field, offset, value)| {
            let span = layout.resolve(field)?;
            (offset < span.width).then_some((span.index + offset, value))
        })
        .collect();
    ObjectFields::from_native_pairs(layout, &pairs)
}

#[test]
fn dialect_packed_bytes_never_borrow_neighbor_meanings() {
    for layout in [FieldLayout::Vanilla5875, FieldLayout::Wrath12340] {
        let wrath = layout == FieldLayout::Wrath12340;
        let fields = fixture(
            layout,
            &[
                (Field::UnitBytes0, 0, 0x0403_0201),
                (
                    Field::UnitBytes1,
                    0,
                    if wrath { 0xa107_9908 } else { 0x0711_0608 },
                ),
                (Field::UnitBytes2, 0, 0x1200_0002),
                (Field::PlayerBytes, 0, 0x1817_1615),
                (Field::PlayerBytes2, 0, 0x0207_0019),
                (Field::PlayerBytes3, 0, 0x1c1b_1a03),
                (Field::PlayerFieldBytes, 0, 0x1e1d_050a),
                (Field::PlayerFieldBytes2, 0, 0x40),
                (Field::PlayerFlags, 0, 0xc31),
            ],
        );
        assert_eq!(fields.unit_race(), Some(1));
        assert_eq!(fields.unit_class(), Some(2));
        assert_eq!(fields.unit_gender(), Some(3));
        assert_eq!(fields.unit_power_type(), 4);
        assert_eq!(fields.unit_stand_state(), 8);
        assert_eq!(fields.unit_loyalty_level(), if wrath { 0 } else { 6 });
        assert_eq!(fields.unit_shapeshift_form(), if wrath { 18 } else { 17 });
        assert!(fields.unit_is_stealthed());
        assert!(fields.unit_is_untrackable());
        assert_eq!(fields.unit_is_ghost_visual(), !wrath);
        assert_eq!(fields.unit_sheath_state(), Some(2));
        assert_eq!(fields.player_skin(), Some(21));
        assert_eq!(fields.player_face(), Some(22));
        assert_eq!(fields.player_hair_style(), Some(23));
        assert_eq!(fields.player_hair_color(), Some(24));
        assert_eq!(fields.player_facial_hair(), Some(25));
        assert_eq!(fields.player_bank_bag_slots_purchased(), Some(7));
        assert_eq!(fields.player_rest_state(), Some(2));
        assert_eq!(fields.player_drunk_byte(), Some(26));
        assert_eq!(fields.player_pvp_rank(), Some(if wrath { 27 } else { 28 }));
        assert_eq!(
            fields.player_pvp_medal(),
            if wrath { None } else { Some(27) }
        );
        assert_eq!(
            fields.player_combo_points(),
            if wrath { None } else { Some(5) }
        );
        assert_eq!(fields.player_action_bar_toggles(), Some(29));
        assert_eq!(fields.player_honor_rank(), Some(30));
        assert_eq!(
            fields.player_honor_rank_bar(),
            if wrath { None } else { Some(64) }
        );
        assert!(fields.player_release_timer_running());
        assert!(fields.player_track_stealthed());
        assert!(fields.player_is_group_leader());
        assert!(fields.player_is_ghost());
        assert!(fields.player_is_resting());
        assert!(fields.player_hides_helm());
        assert!(fields.player_hides_cloak());
        assert!(fields.is_dead_or_ghost());
    }
}

#[test]
fn native_go_and_dynamicobject_fields_do_not_alias_movement_or_parent_rotation() {
    for layout in [FieldLayout::Vanilla5875, FieldLayout::Wrath12340] {
        let wrath = layout == FieldLayout::Wrath12340;
        let fields = fixture(
            layout,
            &[
                (Field::GameobjectState, 0, 2),
                (Field::GameobjectBytes1, 0, 0xabcd_0b02),
                (Field::GameobjectTypeId, 0, 11),
                (Field::GameobjectDynFlags, 0, 0x35),
                (Field::GameobjectDynamic, 0, 0xbeef_0035),
                (Field::GameobjectParentrotation, 0, 8f32.to_bits()),
                (Field::GameobjectRotation, 0, 1f32.to_bits()),
                (Field::GameobjectRotation, 1, 2f32.to_bits()),
                (Field::GameobjectRotation, 2, 3f32.to_bits()),
                (Field::GameobjectRotation, 3, 4f32.to_bits()),
                (Field::GameobjectPosX, 0, 5f32.to_bits()),
                (Field::GameobjectPosY, 0, 6f32.to_bits()),
                (Field::GameobjectPosZ, 0, 7f32.to_bits()),
                (Field::GameobjectFacing, 0, 9f32.to_bits()),
            ],
        );
        assert_eq!(fields.gameobject_state(), Some(2));
        assert_eq!(fields.gameobject_type_id(), 11);
        assert_eq!(fields.gameobject_dynamic_flags(), 0x35);
        assert_eq!(
            fields.gameobject_rotation(),
            if wrath { None } else { Some([1., 2., 3., 4.]) }
        );
        assert_eq!(fields.gameobject_pos_sent(), !wrath);
        let position = fields
            .gameobject_position()
            .map(|(p, o)| ([p.x, p.y, p.z], o));
        assert_eq!(
            position,
            if wrath {
                None
            } else {
                Some(([5., 6., 7.], 9.))
            }
        );
        let dynamic = fixture(
            layout,
            &[
                (Field::DynamicobjectPosX, 0, 10f32.to_bits()),
                (Field::DynamicobjectPosY, 0, 11f32.to_bits()),
                (Field::DynamicobjectPosZ, 0, 12f32.to_bits()),
                (Field::DynamicobjectFacing, 0, 13f32.to_bits()),
                (Field::DynamicobjectCasttime, 0, 99),
            ],
        );
        assert_eq!(
            dynamic.dynamicobject_position(),
            if wrath {
                None
            } else {
                Some(([10., 11., 12.], 13.))
            }
        );
        let created =
            ObjectFields::from_native_pairs(layout, &[]).into_created(ObjectType::GameObject);
        assert_eq!(created.gameobject_state(), Some(0));
        assert_eq!(created.gameobject_rotation(), None);
        assert_eq!(
            ObjectFields::from_native_pairs(layout, &[]).gameobject_state(),
            None
        );
    }
}

#[test]
fn native_array_accessors_obey_spans_and_distinct_elements() {
    for layout in [FieldLayout::Vanilla5875, FieldLayout::Wrath12340] {
        let wrath = layout == FieldLayout::Wrath12340;
        let roots = [
            Field::PlayerInvSlotHead,
            Field::PlayerPackSlot1,
            Field::PlayerBankSlot1,
            Field::PlayerBankbagSlot1,
            Field::PlayerKeyringSlot1,
            Field::PlayerVendorbuybackSlot1,
            Field::ContainerSlot1,
        ];
        let readers: [fn(&ObjectFields, u8) -> Option<u64>; 7] = [
            ObjectFields::player_inv_slot,
            ObjectFields::player_pack_slot,
            ObjectFields::player_bank_slot,
            ObjectFields::player_bank_bag_slot,
            ObjectFields::player_keyring_slot,
            ObjectFields::player_buyback_slot,
            ObjectFields::container_slot,
        ];
        for (root, read) in roots.into_iter().zip(readers) {
            let span = layout.resolve(root).unwrap();
            let pairs: Vec<_> = (0..span.width)
                .map(|i| (span.index + i, 700 + u32::from(i)))
                .collect();
            let fields = ObjectFields::from_native_pairs(layout, &pairs);
            for i in 0..span.width / 2 {
                assert_eq!(
                    read(&fields, i as u8),
                    Some(
                        u64::from(700 + 2 * u32::from(i))
                            | (u64::from(701 + 2 * u32::from(i)) << 32)
                    )
                );
            }
            assert_eq!(read(&fields, (span.width / 2) as u8), None);
        }
        let fields = fixture(
            layout,
            &[
                (Field::PlayerVisibleItems, if wrath { 36 } else { 218 }, 765),
                (
                    Field::PlayerVisibleItems,
                    if wrath { 37 } else { 219 },
                    if wrath { 0x0016_0015 } else { 21 },
                ),
                (Field::PlayerVisibleItems, 220, 22),
                (Field::PlayerVisibleItems, 226, 23),
                (Field::ItemEnchantment, 0, 31),
                (Field::ItemEnchantment, 2, 32),
                (Field::ItemSpellCharges, 4, (-33i32) as u32),
                (Field::PlayerSkillInfo11, 381, 0x0004_0029),
                (Field::PlayerSkillInfo11, 382, 0x0064_0032),
                (Field::PlayerSkillInfo11, 383, 0xfffd_fffe),
                (Field::PlayerBuybackPrice1, 11, 34),
                (Field::PlayerBuybackTimestamp1, 11, 35),
                (Field::PlayerExploredZones1, 63, 36),
            ],
        );
        assert_eq!(fields.player_visible_item_entry(18), Some(765));
        assert_eq!(fields.player_visible_item_entry(19), None);
        assert_eq!(fields.player_visible_item_enchant(18, 0), Some(21));
        assert_eq!(fields.player_visible_item_enchant(18, 1), Some(22));
        assert_eq!(
            fields.player_visible_item_properties(18),
            if wrath { 0 } else { 23 }
        );
        assert_eq!(fields.item_enchant(0), Some(31));
        assert_eq!(fields.item_enchant_charges(0), 32);
        assert_eq!(fields.item_enchant(255), None);
        assert_eq!(fields.item_spell_charges(4), Some(-33));
        assert_eq!(fields.item_spell_charges(5), None);
        let skill = fields.player_skill(127).unwrap();
        assert_eq!(
            (
                skill.skill_id,
                skill.step,
                skill.value,
                skill.max,
                skill.temp_bonus,
                skill.perm_bonus
            ),
            (41, 4, 50, 100, -2, -3)
        );
        assert_eq!(fields.player_skill(128), None);
        assert_eq!(fields.player_buyback_price(11), Some(34));
        assert_eq!(fields.player_buyback_timestamp(11), Some(35));
        assert_eq!(fields.player_buyback_price(12), None);
        assert_eq!(fields.player_buyback_timestamp(12), None);
        assert_eq!(fields.player_explored_zone_slot(63), 36);
        assert_eq!(fields.player_explored_zone_slot(u16::MAX), 0);
    }
}

#[test]
fn unit_composites_and_legacy_only_values_keep_native_semantics() {
    for layout in [FieldLayout::Vanilla5875, FieldLayout::Wrath12340] {
        let wrath = layout == FieldLayout::Wrath12340;
        let fields = fixture(
            layout,
            &[
                (Field::UnitCharm, 0, 41),
                (Field::UnitSummon, 0, 42),
                (Field::UnitSummonedby, 0, 43),
                (Field::UnitCreatedby, 0, 44),
                (Field::UnitPetnumber, 0, 45),
                (Field::UnitPetexperience, 0, 46),
                (Field::UnitPetnextlevelexp, 0, 47),
                (Field::UnitTrainingPoints, 0, 0x0031_0030),
                (Field::UnitDynamicFlags, 0, 0x2d),
                (Field::UnitHealth, 0, 0),
                (Field::UnitMaxHealth, 0, 999),
                (Field::UnitAura, 0, 51),
                (Field::UnitAuraFlags, 0, 0x2),
                (Field::UnitAuraLevels, 0, 52),
                (Field::UnitAuraApplications, 0, 2),
                (Field::UnitVirtualItemSlotDisplay, 2, 53),
                (Field::UnitVirtualItemInfo, 4, 0x0403_0201),
                (Field::UnitVirtualItemInfo, 5, 5),
                (Field::UnitVirtualItemSlotId, 2, 999),
                (Field::UnitAttackPowerMods, 0, 0xfffe_0036),
                (Field::UnitRangedAttackPowerMods, 0, 0xfffc_0037),
            ],
        );
        assert_eq!(fields.unit_pet_guid(), Some(41));
        assert_eq!(fields.unit_owner(OwnerFallback::CreatedBy), Some(44));
        assert_eq!(fields.unit_owner(OwnerFallback::SummonedBy), Some(43));
        assert!(fields.unit_is_pet_or_charm());
        assert_eq!(fields.unit_pet_experience(), (46, 47));
        assert_eq!(
            fields.unit_training_points(),
            if wrath { (0, 0) } else { (49, 48) }
        );
        assert!(fields.unit_is_dead());
        assert!(fields.unit_reads_dead());
        assert!(fields.unit_dynflag_dead());
        assert!(fields.unit_tapped());
        assert!(fields.unit_tapped_by_player());
        assert!(fields.unit_lootable());
        assert_eq!(fields.unit_attack_power_mods(), (54, -2));
        assert_eq!(fields.unit_ranged_attack_power_mods(), (55, -4));
        assert_eq!(
            fields.unit_virtual_item_display(2),
            if wrath { None } else { Some(53) }
        );
        assert_eq!(
            fields.unit_virtual_item_info(2),
            if wrath { None } else { Some((1, 2, 3, 4)) }
        );
        assert_eq!(
            fields.unit_virtual_item_sheath(2),
            if wrath { None } else { Some(5) }
        );
        assert_eq!(fields.unit_virtual_item_display(3), None);
        assert_eq!(fields.unit_virtual_item_info(3), None);
        assert_eq!(fields.unit_virtual_item_sheath(3), None);
        assert_eq!(fields.unit_auras().count(), if wrath { 0 } else { 1 });
        assert_eq!(
            fields.unit_aura_ids().next(),
            if wrath { None } else { Some(51) }
        );
        let aura = fields.unit_aura(0);
        assert_eq!(
            aura.map(|a| (a.spell_id, a.flags, a.level, a.stacks)),
            if wrath { None } else { Some((51, 2, 52, 3)) }
        );
        assert_eq!(fields.unit_aura(48), None);
    }
}

#[test]
fn moved_stats_resistances_and_native_honor_aliases_read_distinct_values() {
    for layout in [FieldLayout::Vanilla5875, FieldLayout::Wrath12340] {
        let wrath = layout == FieldLayout::Wrath12340;
        let positives = if wrath {
            [
                Field::UnitPosstat0,
                Field::UnitPosstat1,
                Field::UnitPosstat2,
                Field::UnitPosstat3,
                Field::UnitPosstat4,
            ]
        } else {
            [
                Field::PlayerPosstat0,
                Field::PlayerPosstat1,
                Field::PlayerPosstat2,
                Field::PlayerPosstat3,
                Field::PlayerPosstat4,
            ]
        };
        let negatives = if wrath {
            [
                Field::UnitNegstat0,
                Field::UnitNegstat1,
                Field::UnitNegstat2,
                Field::UnitNegstat3,
                Field::UnitNegstat4,
            ]
        } else {
            [
                Field::PlayerNegstat0,
                Field::PlayerNegstat1,
                Field::PlayerNegstat2,
                Field::PlayerNegstat3,
                Field::PlayerNegstat4,
            ]
        };
        let stats = [
            Field::UnitStat0,
            Field::UnitStat1,
            Field::UnitStat2,
            Field::UnitStat3,
            Field::UnitStat4,
        ];
        let mut values = Vec::new();
        for i in 0..5 {
            values.extend([
                (positives[i], 0, 61 + i as u32),
                (negatives[i], 0, (-71 - i as i32) as u32),
                (stats[i], 0, 81 + i as u32),
            ]);
        }
        let pos = if wrath {
            Field::UnitResistancebuffmodspositive
        } else {
            Field::PlayerResistancebuffmodspositive
        };
        let neg = if wrath {
            Field::UnitResistancebuffmodsnegative
        } else {
            Field::PlayerResistancebuffmodsnegative
        };
        for i in 0..7 {
            values.extend([
                (pos, i, 91 + u32::from(i)),
                (neg, i, (-101 - i as i32) as u32),
                (Field::UnitResistances, i, 111 + u32::from(i)),
                (Field::PlayerModDamageDonePos, i, 121 + u32::from(i)),
                (Field::PlayerModDamageDoneNeg, i, (-131 - i as i32) as u32),
                (
                    Field::PlayerModDamageDonePct,
                    i,
                    (1.5 + f32::from(i)).to_bits(),
                ),
            ]);
        }
        values.extend([
            (Field::PlayerLifetimeHonorbaleKills, 0, 141),
            (Field::PlayerLifetimeHonorableKills, 0, 142),
        ]);
        let fields = fixture(layout, &values);
        for i in 0..5 {
            assert_eq!(fields.player_posstat(i), Some(61 + i as i32));
            assert_eq!(fields.player_negstat(i), Some(-71 - i as i32));
            assert_eq!(fields.unit_stat(i), Some(81 + u32::from(i)));
        }
        for i in 0..7 {
            assert_eq!(fields.player_resistance_buff_pos(i), Some(91 + i as i32));
            assert_eq!(fields.player_resistance_buff_neg(i), Some(-101 - i as i32));
            assert_eq!(fields.unit_resistance(i), Some(111 + i as i32));
            assert_eq!(fields.player_mod_damage_done_pos(i), Some(121 + i as i32));
            assert_eq!(fields.player_mod_damage_done_neg(i), Some(-131 - i as i32));
            assert_eq!(
                fields.player_mod_damage_done_pct(i),
                Some(1.5 + f32::from(i))
            );
        }
        assert_eq!(fields.player_posstat(5), None);
        assert_eq!(fields.player_negstat(5), None);
        assert_eq!(fields.unit_stat(5), None);
        assert_eq!(fields.player_resistance_buff_pos(7), None);
        assert_eq!(fields.player_resistance_buff_neg(7), None);
        assert_eq!(fields.unit_resistance(7), None);
        assert_eq!(
            fields.player_lifetime_honorable_kills(),
            Some(if wrath { 142 } else { 141 })
        );
    }
}

#[test]
fn corpse_quest_resource_and_object_accessors_preserve_both_native_shapes() {
    for layout in [FieldLayout::Vanilla5875, FieldLayout::Wrath12340] {
        let wrath = layout == FieldLayout::Wrath12340;
        let mut values = vec![
            (Field::CorpseBytes1, 0, 0x0403_0200),
            (Field::CorpseBytes2, 0, 0x0807_0605),
            (Field::CorpseFlags, 0, 0x3d),
            (Field::CorpseDynamicFlags, 0, 1),
            (Field::CorpseItem, 18, 0x1300_0065),
            (Field::ObjectType, 0, 0x17),
            (Field::UnitMaxHealth, 0, 999),
            (Field::UnitHealth, 0, 777),
            (Field::UnitBytes0, 0, 0x0300_0000),
        ];
        let last = if wrath { 24 } else { 19 };
        let stride = if wrath { 5 } else { 3 };
        values.extend([
            (Field::PlayerQuestLog, stride * last, 301),
            (
                Field::PlayerQuestLog,
                stride * last + 1,
                if wrath { 2 } else { 0x020c_2041 },
            ),
        ]);
        if wrath {
            values.extend([
                (Field::PlayerQuestLog, stride * last + 2, 0x0082_0081),
                (Field::PlayerQuestLog, stride * last + 3, 0x0084_0083),
                (Field::PlayerQuestLog, stride * last + 4, 302),
            ]);
        } else {
            values.push((Field::PlayerQuestLog, stride * last + 2, 302));
        }
        for i in 0..if wrath { 7 } else { 5 } {
            values.extend([
                (Field::UnitPower, i, 401 + u32::from(i)),
                (Field::UnitMaxPower, i, 501 + u32::from(i)),
            ]);
        }
        let fields = fixture(layout, &values);
        let corpse = fixture(
            layout,
            &[
                (Field::CorpseBytes1, 0, 0x0403_0200),
                (Field::CorpseBytes2, 0, 0x0807_0605),
                (Field::CorpseFlags, 0, 0x3d),
                (Field::CorpseDynamicFlags, 0, 1),
                (Field::CorpseItem, 18, 0x1300_0065),
            ],
        );
        let look = corpse.corpse_look().unwrap();
        assert_eq!(
            (
                look.race,
                look.sex,
                look.skin,
                look.face,
                look.hair_style,
                look.hair_color,
                look.facial_hair
            ),
            (2, 3, 4, 5, 6, 7, 8)
        );
        assert_eq!(corpse.corpse_race(), 2);
        assert_eq!(corpse.corpse_item(18), Some((101, 19)));
        assert_eq!(corpse.corpse_item(19), None);
        assert!(corpse.corpse_is_bones());
        assert!(corpse.corpse_hides_helm());
        assert!(corpse.corpse_hides_cloak());
        assert!(corpse.corpse_pvp_insignia());
        assert!(corpse.corpse_lootable());
        assert!(fields.is_container());
        assert_eq!(fields.object_type(), Some(ObjectType::Player));
        assert_eq!(fields.player_quest_log_slots(), if wrath { 25 } else { 20 });
        let quest = fields.player_quest_log(last as u8).unwrap();
        assert_eq!((quest.quest_id, quest.state, quest.timer), (301, 2, 302));
        assert_eq!(
            quest.counters,
            if wrath {
                [129, 130, 131, 132]
            } else {
                [1, 1, 2, 3]
            }
        );
        assert_eq!(fields.player_quest_log(last as u8 + 1), None);
        assert_eq!(fields.unit_base_attack_time(2), None);
        assert_eq!(fields.unit_base_attack_time(0), None);
        assert_eq!(fields.unit_power_slots(), if wrath { 7 } else { 5 });
        for i in 0..fields.unit_power_slots() {
            assert_eq!(fields.unit_power(i), Some(401 + u32::from(i)));
            assert_eq!(fields.unit_max_power(i), Some(501 + u32::from(i)));
        }
        assert_eq!(fields.unit_power(fields.unit_power_slots()), None);
        assert_eq!(fields.unit_max_power(fields.unit_power_slots()), None);
        assert_eq!(fields.unit_shown_health(), Some(777));
        assert_eq!(fields.unit_shown_power(3), Some(404));
        assert_eq!(fields.unit_shown_max_power(3), Some(504));
        assert_eq!(
            fields.wrath_powers().map(|p| p[6]),
            if wrath { Some((407, 507)) } else { None }
        );
        assert!(!fields.is_empty());
        assert!(fields.raw_fields().count() > 0);
        assert_eq!(fields.created_as(), None);
        let created = fields.clone().into_created(ObjectType::Player);
        assert_eq!(created.created_as(), Some(ObjectType::Player));
        assert_eq!(created.layout(), layout);
        assert!(created.field_present(Field::ObjectType, 0));
        assert_eq!(created.get_field_guid(Field::UnitSummon, 0), None);
        assert_eq!(created.get(Field::ObjectType), Some(0x17));
        assert_eq!(created.get_at(Field::ObjectType, 1), None);
        assert_eq!(created.get_float(Field::ObjectScaleX, 0), Some(0.));
        let mut merged = created.clone();
        let delta = fixture(layout, &[(Field::UnitHealth, 0, 222)]);
        let mut edges = Vec::new();
        merged.merge_diff(delta, |index, old, new| edges.push((index, old, new)));
        assert_eq!(merged.unit_health(), Some(222));
        assert_eq!(edges.len(), 1);
        merged.merge(fixture(layout, &[(Field::UnitHealth, 0, 333)]));
        assert_eq!(merged.unit_health(), Some(333));
    }
}

/// Adding a public reader requires adding it to a dual-layout fixture, not merely compiling it.
#[test]
fn public_accessor_fixture_census_has_no_unreviewed_reader() {
    let sources = [
        include_str!("player.rs"),
        include_str!("unit.rs"),
        include_str!("mod.rs")
            .split("impl ObjectFields {")
            .nth(1)
            .unwrap(),
    ];
    let fixtures = include_str!("accessor_tests.rs");
    for source in sources {
        for declaration in source.split("pub fn ").skip(1) {
            let (name, signature) = declaration.split_once('(').unwrap();
            if !signature.starts_with("&self") && !signature.starts_with("&mut self") {
                continue;
            }
            assert!(
                fixtures.contains(&format!(".{name}("))
                    || fixtures.contains(&format!("ObjectFields::{name}")),
                "{name} needs a dual-layout fixture"
            );
        }
    }
}

#[test]
fn pet_permissions_use_only_their_native_dialect_field_and_bit() {
    for layout in [FieldLayout::Vanilla5875, FieldLayout::Wrath12340] {
        let absent = fixture(layout, &[]);
        assert!(!absent.unit_pet_can_be_renamed());
        assert!(!absent.unit_pet_can_be_abandoned());
        for field in [Field::UnitFlags, Field::UnitBytes2] {
            for bit in 0..32 {
                let fields = fixture(layout, &[(field, 0, 1u32 << bit)]);
                let (source, rename_bit, abandon_bit) = match layout {
                    FieldLayout::Vanilla5875 => (Field::UnitFlags, 4, 5),
                    FieldLayout::Wrath12340 => (Field::UnitBytes2, 16, 17),
                };
                assert_eq!(
                    fields.unit_pet_can_be_renamed(),
                    field == source && bit == rename_bit,
                    "rename {layout:?} {field:?} bit {bit}"
                );
                assert_eq!(
                    fields.unit_pet_can_be_abandoned(),
                    field == source && bit == abandon_bit,
                    "abandon {layout:?} {field:?} bit {bit}"
                );
            }
        }
        let both = fixture(
            layout,
            &[
                (Field::UnitFlags, 0, u32::MAX),
                (Field::UnitBytes2, 0, u32::MAX),
            ],
        );
        assert!(both.unit_pet_can_be_renamed());
        assert!(both.unit_pet_can_be_abandoned());
        let zero = fixture(
            layout,
            &[(Field::UnitFlags, 0, 0), (Field::UnitBytes2, 0, 0)],
        );
        assert!(!zero.unit_pet_can_be_renamed());
        assert!(!zero.unit_pet_can_be_abandoned());
    }
}
