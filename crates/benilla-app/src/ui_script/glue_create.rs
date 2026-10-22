//! The stock CharacterCreate on the glue VM (`WOW_STOCK_GLUE=1`): the host side of
//! [`benilla_ui::script::glue_create`]. Entering the screen seeds the API's catalog from the
//! installed data; each frame the VM's selection dresses the create booth, `CreateCharacter`
//! goes out through the native create's wire path, and its result comes back as the engine's
//! status events (`OPEN_STATUS_DIALOG`, `UPDATE_STATUS_DIALOG`, `CLOSE_STATUS_DIALOG`). The screen
//! itself switches in Lua (`SetGlueScreen`), which this follows into the client state. The native
//! create screen stands down meanwhile (`char_create::native_create`).

use std::collections::HashMap;

use benilla_formats::{CharCreateCatalog, ChrClasses};
use benilla_protocol::{CharAction, CharCreateReq};
use benilla_ui::script::glue_create::{
    GlueCreateCatalog, GlueCreateClass, GlueCreateRace, GlueCreateRequest,
};
use benilla_ui::script::{ScriptValue, UiScript};
use bevy::prelude::*;

use crate::char_create::{ALLIANCE, CLASS_SLOTS, HORDE};
use crate::char_select::ClientState;
use crate::glue_strings::GlueStrings;
use crate::portrait::{CreateLook, GlueLook, GluePreview};

/// The stock create screen's systems.
pub(crate) struct StockCreatePlugin;

impl Plugin for StockCreatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StockCreate>()
            .add_systems(
                OnEnter(ClientState::CharCreate),
                enter_stock_create.run_if(super::glue::stock_glue_active),
            )
            .add_systems(
                Update,
                (
                    follow_glue_screen.run_if(super::glue::stock_glue_active),
                    drive_stock_create.run_if(stock_create_up),
                )
                    .chain()
                    // Where the native create runs: among the glue screens, before the booth.
                    .in_set(crate::glue::GlueScreens::Create)
                    .before(crate::glue::GlueVisuals)
                    .after(super::UiInput)
                    .after(benilla_world::schedule::WorldStage::Net),
            );
    }
}

/// Run condition: the stock glue hosts the create screen.
pub(crate) fn stock_create_up(state: Option<Res<State<ClientState>>>) -> bool {
    super::glue::stock_glue_active() && state.is_some_and(|s| *s.get() == ClientState::CharCreate)
}

/// The create in flight: the name sent, until its result.
#[derive(Resource, Default)]
pub(crate) struct StockCreate {
    pending: Option<String>,
}

/// The create list: Alliance then Horde, ascending race id, the order `GetAvailableRaces`
/// (`0x4e1880`) walks and `CharacterCreate.xml`'s two button columns show.
fn create_races() -> impl Iterator<Item = u8> {
    ALLIANCE.into_iter().chain(HORDE)
}

/// The `UI_<name>` stage a pair stands on, build 12340's select-screen rule
/// (`GetSelectBackgroundModel` `0x4e3620`) as `portrait`'s `glue_stage` transcribes it: a Death
/// Knight on its class's, Gnome on Dwarf's, Troll on Orc's, else the race's file. Kept here until
/// that function is reachable from outside `portrait` (Wire A's B3 moves it).
fn stage(race: u8, class: u8) -> &'static str {
    if class == 6 {
        return "DeathKnight";
    }
    match race {
        2 | 8 => "Orc",
        3 | 7 => "Dwarf",
        4 => "NightElf",
        5 => "Scourge",
        6 => "Tauren",
        10 => "BloodElf",
        11 => "Draenei",
        _ => "Human",
    }
}

/// `GetSelectedClass`' role bits (`0x552940`'s 2, 4 and 8). Inferred: the mask is undecompiled,
/// so these are the classes' 3.3.5 roles (tank, healer, damage); stock glue reads none of them.
fn roles(class: u8) -> [bool; 3] {
    [
        matches!(class, 1 | 2 | 6 | 11),
        matches!(class, 2 | 5 | 7 | 11),
        true,
    ]
}

/// The catalog the glue API answers from: names, files and tokens off `ChrRaces` and
/// `ChrClasses`, the `CharBaseInfo` pairs, the dial ranges, and each pair's stage. A Death Knight
/// is enabled only with a level-55 character on the roster (`DEATHKNIGHT_DISABLED`, as the native
/// screen gates it; AzerothCore enforces it too).
pub(crate) fn catalog(
    cat: &CharCreateCatalog,
    classes: Option<&ChrClasses>,
    death_knight: bool,
) -> GlueCreateCatalog {
    let tag = |race: u8| {
        if ALLIANCE.contains(&race) {
            "Alliance"
        } else {
            "Horde"
        }
    };
    let races = create_races()
        .map(|id| {
            let token = |sex| {
                cat.facial_hair_customization(id, sex)
                    .unwrap_or("NORMAL")
                    .into()
            };
            GlueCreateRace {
                id,
                names: [0, 1].map(|sex| cat.race_name(id, sex).unwrap_or_default().to_string()),
                file: cat.race_file(id).unwrap_or_default().into(),
                enabled: true,
                // enUS: the faction group's localized name is its internal one.
                faction: (tag(id).into(), tag(id).into()),
                hair: cat.hair_customization(id).unwrap_or("NORMAL").into(),
                facial_hair: [token(0), token(1)],
            }
        })
        .collect();
    // `ChrClasses` in id order, the rows `GetAvailableClasses` walks, beside their ids.
    let names = classes.map(ChrClasses::localized_names).unwrap_or_default();
    let classes = CLASS_SLOTS
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            let (file, male, female) = names.get(i).cloned().unwrap_or_default();
            GlueCreateClass {
                id,
                names: [male, female],
                file,
                enabled: id != 6 || death_knight,
                roles: roles(id),
            }
        })
        .collect();
    let mut valid = Vec::new();
    let mut dials = HashMap::new();
    let mut backgrounds = HashMap::new();
    for race in create_races() {
        for class in cat.classes_for_race(race) {
            valid.push((race, class));
            backgrounds.insert((race, class), stage(race, class).into());
        }
        for sex in [0, 1] {
            if let Some(r) = cat.ranges(race, sex) {
                dials.insert(
                    (race, sex),
                    [r.skin, r.face, r.hair_style, r.hair_color, r.facial_hair],
                );
            }
        }
    }
    GlueCreateCatalog {
        races,
        classes,
        valid,
        dials,
        backgrounds,
    }
}

/// Entering the create screen on the stock glue: seed the catalog. The Lua side resets the
/// selection itself (`CharacterCreate_OnShow` calls `ResetCharCustomize`).
fn enter_stock_create(
    script: Option<NonSendMut<UiScript>>,
    catalog: Option<Res<crate::entities::CharCreate>>,
    classes: Option<Res<crate::chr_classes::ChrClassTable>>,
    roster: Res<crate::char_select::Roster>,
    time: Res<Time>,
    mut pending: ResMut<StockCreate>,
) {
    let (Some(mut script), Some(catalog)) = (script, catalog) else {
        return;
    };
    let death_knight = roster.chars.iter().any(|c| c.level >= 55);
    script.set_glue_create_catalog(self::catalog(
        &catalog.0,
        classes.as_deref().map(|c| &c.0),
        death_knight,
    ));
    script.seed_glue_create_rng(time.elapsed().as_nanos() as u64 ^ 0x5DEE_CE66);
    pending.pending = None;
}

/// The Lua side switches screens (`SetGlueScreen` → `SetCurrentScreen`); the client state
/// follows: `"charcreate"` from select, `"charselect"` back.
fn follow_glue_screen(
    script: Option<NonSend<UiScript>>,
    state: Res<State<ClientState>>,
    mut next: ResMut<NextState<ClientState>>,
) {
    let Some(script) = script else { return };
    match (state.get(), script.glue_current_screen().as_str()) {
        (ClientState::CharSelect, "charcreate") => next.set(ClientState::CharCreate),
        (ClientState::CharCreate, "charselect") => next.set(ClientState::CharSelect),
        _ => {}
    }
}

fn s(text: &str) -> ScriptValue {
    ScriptValue::Str(text.into())
}

/// The create screen's calls, the booth and the create's result.
#[allow(clippy::too_many_arguments)]
pub(crate) fn drive_stock_create(
    script: Option<NonSendMut<UiScript>>,
    mut preview: ResMut<GluePreview>,
    pick: Option<Res<crate::net::CharPick>>,
    strings: Option<Res<GlueStrings>>,
    mut roster: ResMut<crate::char_select::Roster>,
    mut results: MessageReader<crate::net::WrathCharActionResultMessage>,
    mut pending: ResMut<StockCreate>,
) {
    let Some(mut script) = script else { return };
    let empty = GlueStrings::default();
    let strings = strings.as_deref().unwrap_or(&empty);
    let sel = script.glue_create_selection();
    for request in script.take_glue_create_requests() {
        match request {
            GlueCreateRequest::Create(name) if pending.pending.is_none() => {
                let req = CharCreateReq {
                    name: name.clone(),
                    race: sel.race,
                    class: sel.class,
                    gender: sel.sex,
                    skin: sel.dials[0],
                    face: sel.dials[1],
                    hair_style: sel.dials[2],
                    hair_color: sel.dials[3],
                    facial_hair: sel.dials[4],
                };
                if let Some(pick) = &pick {
                    let _ = pick.0.send(crate::net::CharRequest::Create(req));
                }
                pending.pending = Some(name);
                let text = strings.text("CHAR_CREATE_IN_PROGRESS", "Creating character...");
                script.fire_event("OPEN_STATUS_DIALOG", vec![s("CANCEL"), s(text)]);
            }
            // A second Create while one is in flight, the paid entry (B4's paid services are the
            // native screen's) and the frame and scene asks the booth answers every frame anyway.
            GlueCreateRequest::Create(_)
            | GlueCreateRequest::CustomizeFrame(_)
            | GlueCreateRequest::UpdateScene
            | GlueCreateRequest::CustomizeExisting(_) => {}
        }
    }
    for msg in results.read() {
        if msg.action != CharAction::Create {
            continue;
        }
        let name = pending.pending.take().unwrap_or_default();
        if msg.code == benilla_protocol::wrath_world::CHAR_CREATE_SUCCESS {
            roster.note_created(name);
            script.fire_event("CLOSE_STATUS_DIALOG", vec![]);
            // Back to the list, as the engine does on success.
            script.fire_event("SET_GLUE_SCREEN", vec![s("charselect")]);
        } else {
            let key =
                crate::char_select::wrath_char_result_key(msg.code).unwrap_or("CHAR_CREATE_FAILED");
            let text = strings.text(key, key).to_string();
            let okay = strings.text("OKAY", "Okay").to_string();
            script.fire_event("UPDATE_STATUS_DIALOG", vec![s(&text), s(&okay)]);
        }
    }
    // The booth wears the selection; the stage is the glue host's, from `SetBackgroundModel`'s
    // `SetCharCustomizeBackground` (`GetCreateBackgroundModel`).
    preview.look = Some(GlueLook::Create(CreateLook {
        race: sel.race,
        sex: sel.sex,
        class: sel.class,
        skin: sel.dials[0],
        face: sel.dials[1],
        hair_style: sel.dials[2],
        hair_color: sel.dials[3],
        facial_hair: sel.dials[4],
    }));
    preview.yaw = sel.facing;
}

#[cfg(test)]
#[path = "glue_create_tests.rs"]
mod tests;
