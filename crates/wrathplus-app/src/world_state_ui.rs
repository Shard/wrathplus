//! The always-up world-state readout behind stock `WorldStateFrame.xml`: joins the rows
//! `WorldStateUI.dbc` displays with the states the server sent ([`crate::world_state`]).
//!
//! Build 12340's list builder (`0x5487d0`) walks the whole DBC and admits a row when:
//! 1. `MapID` is -1 or the scope's map;
//! 2. `AreaID` is 0 or the scope's zone or area;
//! 3. its phase mask is 0 or shares a bit with the player's;
//! 4. `Type` is 0, 1 or 3; 2 (battleground scoreboard columns) never.
//!
//! The world-PvP rows (`Type` 1) are always listed: `WorldStateFrame.lua` filters them itself
//! through `WORLD_PVP_OBJECTIVES_DISPLAY` and `IsSubZonePVPPOI`. While any listed row is a timer
//! (`Type` 3), `WORLD_STATE_UI_TIMER_UPDATE` fires every second (`0x5488f0`).
//!
//! The scope is the server's last `SMSG_INIT_WORLD_STATES`, not the player's position.
//! Unresolved, labelled: the init's area word is dropped by the decoder, so a row keyed to a
//! subzone is admitted only by its zone; the phase mask is not modelled (every row passes).

use bevy::ecs::system::NonSendMut;
use bevy::prelude::*;

use wrathplus_formats::{WorldStateUiCatalog, WorldStateUiRow};
use wrathplus_ui::script::{UiScript, WorldStateUiView};

use wrathplus_assets::{AssetSet, LockRecover, WorldAssets};

use crate::world_state::WorldStates;

/// The shared `WorldStateUI.dbc` catalog; absent, the readout is empty.
#[derive(Resource)]
pub(crate) struct WorldStateUiRes(pub(crate) WorldStateUiCatalog);

fn load_world_state_ui(mut commands: Commands, assets: Option<Res<WorldAssets>>) {
    let Some(assets) = assets else { return };
    let loaded = {
        let mut chain = assets.chain.lock_recover();
        wrathplus_formats::load_world_state_ui_catalog(&mut chain)
    };
    match loaded {
        Ok(cat) => {
            info!("world-state UI: {} rows in WorldStateUI.dbc", cat.len());
            commands.insert_resource(WorldStateUiRes(cat));
        }
        Err(e) => warn!("world-state UI: WorldStateUI.dbc failed — no readout: {e:#}"),
    }
}

/// Expands a `WorldStateUI` label (`0x508560`), a different expander from [`crate::npc_text`]'s,
/// sharing only the world-state getter. The grammar is `%<digits>W|w` alone, printed `%d`; any
/// other `%` emits a literal `%`, consumes its digits and leaves the next character as text.
/// Output is truncated at the reference's 256-byte limit.
fn expand(text: &str, states: &WorldStates) -> String {
    /// The `strncat` bound: `0x508560`'s caller passes `0x100`.
    const LIMIT: usize = 0x100 - 1;

    fn push(s: &str, out: &mut String) {
        for ch in s.chars() {
            if out.len() + ch.len_utf8() > LIMIT {
                return;
            }
            out.push(ch);
        }
    }

    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            let ch = text[i..].chars().next().expect("char boundary");
            push(ch.encode_utf8(&mut [0u8; 4]), &mut out);
            i += ch.len_utf8();
            continue;
        }
        let digits_at = i + 1;
        let mut j = digits_at;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        match bytes.get(j) {
            Some(b'w' | b'W') => {
                // `atoi`: an empty run is key 0.
                let id: u32 = text[digits_at..j].parse().unwrap_or(0);
                push(&states.get(id).to_string(), &mut out);
                i = j + 1;
            }
            _ => {
                push("%", &mut out);
                i = j;
            }
        }
    }
    out
}

/// The admitted rows for the current scope, resolved as `GetWorldStateUIInfo` answers them.
fn build(catalog: &WorldStateUiCatalog, states: &WorldStates) -> Vec<WorldStateUiView> {
    let Some((map, area)) = states.scope() else {
        // Before an init the reference never rebuilds, and its scope globals read -1.
        return Vec::new();
    };
    catalog
        .rows()
        .filter(|(_, row)| admits(row, map, area))
        .map(|(_, row)| resolve(row, states))
        .collect()
}

fn admits(row: &WorldStateUiRow, map: u32, area: u32) -> bool {
    let map_ok = row.map_id == u32::MAX || row.map_id == map;
    let area_ok = row.area_id == 0 || row.area_id == area;
    map_ok && area_ok && matches!(row.ui_type, 0 | 1 | 3)
}

/// `GetWorldStateUIInfo` for one row (`0x548d40`): only the text is expanded, and the extended
/// ids answer their states' values.
fn resolve(row: &WorldStateUiRow, states: &WorldStates) -> WorldStateUiView {
    WorldStateUiView {
        ui_type: row.ui_type,
        // No `StateVariable` answers 1 (`0x4c5ad8`).
        ui_state: match row.state_variable {
            0 => 1,
            id => states.get(id),
        },
        text: expand(&row.text, states),
        icon: row.icon.clone(),
        dynamic_icon: row.dynamic_icon.clone(),
        tooltip: row.tooltip.clone(),
        dynamic_tooltip: row.dynamic_tooltip.clone(),
        extended_ui: row.extended_ui.clone(),
        extended_ui_state: row.extended_ui_state.map(|id| states.get(id)),
    }
}

/// Pushes the readout when a world-state packet arrives, and ticks the timer rows.
fn feed_world_state_ui(
    script: Option<NonSendMut<UiScript>>,
    catalog: Option<Res<WorldStateUiRes>>,
    states: Res<WorldStates>,
    clock: Res<crate::ui_script::UiClock>,
    mut last: Local<crate::ui_script::VmMemo<Option<u64>>>,
    mut timer: Local<crate::ui_script::VmMemo<(bool, f64)>>,
) {
    let (Some(mut script), Some(catalog)) = (script, catalog) else {
        return;
    };
    let key = states.generation();
    if *last.get(&script) != Some(key) {
        *last.get(&script) = Some(key);
        let rows = build(&catalog.0, &states);
        timer.get(&script).0 = rows.iter().any(|r| r.ui_type == 3);
        script.set_world_state_ui(rows);
    }
    // `0x5488f0`: once a second while a timer row is listed.
    let (has_timer, next) = *timer.get(&script);
    if has_timer && clock.ui_now >= next {
        timer.get(&script).1 = clock.ui_now + 1.0;
        script.fire_event("WORLD_STATE_UI_TIMER_UPDATE", Vec::new());
    }
}

pub(crate) struct WorldStateUiPlugin;

/// `showBattlefieldMinimap` (12340 `0x51ee3c`, default "0"): the zone map's toggle writes "1" in a
/// battleground and "2" elsewhere (Blizzard_BattlefieldMinimap.lua:19-36), and the world-state
/// frame reads it (`WorldStateFrame.lua:378`).
pub(crate) const CVARS: &[crate::cvars::Registered] =
    &[crate::cvars::same("showBattlefieldMinimap", "0")];

impl Plugin for WorldStateUiPlugin {
    fn build(&self, app: &mut App) {
        crate::cvars::RegisterCvars::register_cvars(app, CVARS);
        app.add_systems(Startup, load_world_state_ui.after(AssetSet::Open))
            .add_systems(Update, feed_world_state_ui.after(crate::ui_script::UiInput));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(map_id: u32, area_id: u32, ui_type: u32) -> WorldStateUiRow {
        WorldStateUiRow {
            map_id,
            area_id,
            icon: String::new(),
            text: String::new(),
            tooltip: String::new(),
            state_variable: 0,
            ui_type,
            dynamic_icon: String::new(),
            dynamic_tooltip: String::new(),
            extended_ui: String::new(),
            extended_ui_state: [0; 3],
        }
    }

    #[test]
    fn the_macro_expands_world_state_values() {
        let mut states = WorldStates::default();
        states.write(&[(2327, 3), (2328, 1)]);
        assert_eq!(
            expand("Towers Controlled: %2327w", &states),
            "Towers Controlled: 3"
        );
        assert_eq!(expand("%2327W/%2328w", &states), "3/1");
        assert_eq!(
            expand("Bases: %1779w  Resources: %1776w/%1780w", &states),
            "Bases: 0  Resources: 0/0",
            "an un-received state reads 0, never blank"
        );
        assert_eq!(
            expand("Graveyards Assaulted", &states),
            "Graveyards Assaulted"
        );
        assert_eq!(expand("", &states), "");
    }

    #[test]
    fn a_top_bit_value_prints_negative() {
        let mut states = WorldStates::default();
        states.write(&[(2327, 0xFFFF_FFFF)]);
        assert_eq!(expand("%2327w", &states), "-1");
    }

    /// Unreachable on shipped data.
    #[test]
    fn a_malformed_macro_emits_a_literal_percent() {
        let states = WorldStates::default();
        assert_eq!(expand("100% done", &states), "100% done");
        assert_eq!(expand("%d", &states), "%d");
        assert_eq!(expand("%", &states), "%");
        assert_eq!(
            expand("%123x", &states),
            "%x",
            "the digits scanned ahead of the bad letter are consumed either way"
        );
    }

    #[test]
    fn the_output_is_capped_at_the_reference_buffer() {
        let states = WorldStates::default();
        let long = "a".repeat(600);
        assert_eq!(expand(&long, &states).len(), 0xFF);
    }

    #[test]
    fn the_builder_gates_admit_the_right_rows() {
        // Eastern Plaguelands: map 0, area 139.
        assert!(admits(&row(0, 139, 0), 0, 139));
        assert!(!admits(&row(0, 139, 0), 0, 1377), "wrong area");
        assert!(!admits(&row(0, 139, 0), 1, 139), "wrong map");
        assert!(admits(&row(0, 0, 0), 0, 139), "area 0 is a wildcard");
        assert!(
            admits(&row(u32::MAX, 139, 0), 5, 139),
            "map -1 is a wildcard"
        );
        assert!(
            admits(&row(0, 139, 1), 0, 139),
            "world PvP: the Lua filters it"
        );
        assert!(admits(&row(0, 139, 3), 0, 139), "a timer row");
        assert!(
            !admits(&row(0, 139, 2), 0, 139),
            "a scoreboard column is never an always-up row"
        );
    }

    #[test]
    fn nothing_shows_before_the_first_init() {
        let catalog = WorldStateUiCatalog::from_rows(vec![(136, row(0, 139, 0))]);
        let states = WorldStates::default();
        assert!(build(&catalog, &states).is_empty());
    }

    #[test]
    fn a_row_without_a_state_variable_reads_one() {
        let states = WorldStates::default();
        assert_eq!(resolve(&row(0, 0, 0), &states).ui_state, 1);

        let mut with_state = row(0, 0, 0);
        with_state.state_variable = 2339;
        assert_eq!(
            resolve(&with_state, &states).ui_state,
            0,
            "a state that HAS an id and reads 0 is 0"
        );
        let mut states = WorldStates::default();
        states.write(&[(2339, 1)]);
        assert_eq!(resolve(&with_state, &states).ui_state, 1);
    }

    #[test]
    fn the_extended_ui_ids_are_resolved_to_values() {
        let mut states = WorldStates::default();
        states.write(&[(2427, 42), (2428, 7)]);
        let mut r = row(0, 139, 1);
        r.extended_ui = "CAPTUREPOINT".into();
        r.extended_ui_state = [2427, 2428, 0];
        assert_eq!(resolve(&r, &states).extended_ui_state, [42, 7, 0]);
    }

    /// Against the install's table: Warsong Gulch's always-up rows, and no scoreboard column. The
    /// 3.3.5 rows carry `AreaID` 3277 (the map's own area, which AzerothCore's init sends as the
    /// zone); its area-0 rows are the scoreboard's `Type` 2 columns.
    #[test]
    fn the_real_table_builds_the_warsong_gulch_readout() {
        let data = wrathplus_formats::wow_data_or_skip!();
        let profile = wrathplus_formats::DataProfile::from_env().expect("profile");
        let mut chain = wrathplus_formats::Chain::open_profile(&data, &profile).expect("chain");
        let catalog =
            wrathplus_formats::load_world_state_ui_catalog(&mut chain).expect("WorldStateUI");
        let mut wsg = WorldStates::default();
        wsg.init_scope(489, 3277);
        wsg.write(&[(1581, 2), (1582, 1)]);
        let rows = build(&catalog, &wsg);
        assert!(rows.len() >= 2, "{} rows", rows.len());
        assert!(rows.iter().all(|r| matches!(r.ui_type, 0 | 1 | 3)));
        assert!(rows.iter().any(|r| r.dynamic_icon.contains("Flag")));
        let mut elsewhere = WorldStates::default();
        elsewhere.init_scope(0, 12); // Elwynn Forest
        assert!(build(&catalog, &elsewhere).iter().all(|r| r.ui_type != 2));
    }

    /// An init clears the table.
    #[test]
    fn an_init_forgets_the_previous_zone() {
        let mut states = WorldStates::default();
        states.init_scope(0, 139);
        states.write(&[(2327, 3)]);
        assert_eq!(states.get(2327), 3);

        states.init_scope(0, 12);
        assert_eq!(states.get(2327), 0, "the previous zone's key is gone");
        assert_eq!(states.scope(), Some((0, 12)));
    }
}
