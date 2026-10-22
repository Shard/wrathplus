//! The CharacterCreate glue API, build 12340's `0x4dfc90`..`0x4e2470` (registered at `0xac4228`),
//! installed on the glue VM beside [`super::glue`]. `CharacterCreate.lua` reads the selection back
//! inside the handler that set it (`SetSelectedRace(i)`, then `GetAvailableClasses()`), so the VM
//! owns the selection and applies the rules at once, from a catalog App seeds; App reads the
//! selection to dress the preview and drains [`GlueCreateRequest`]s.
//!
//! The setters' inner rules (`0x4e20b0` race, `0x4e1740` class, `0x4e1540` sex, `0x4e01f0` cycle,
//! `0x4e17f0` randomize, `0x4e1fd0` reset, `0x4dffe0` race/class) are not decompiled: they follow
//! the native create screen's transcription (`char_create`), labelled where it guesses.

use std::collections::HashMap;

use mlua::Lua;

/// One create-list race, as `GetAvailableRaces` (`0x4e1880`) and its neighbours read it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlueCreateRace {
    pub id: u8,
    /// `ChrRaces`' name by sex (0 male, 1 female), which `0x715970` picks by the selected sex.
    pub names: [String; 2],
    /// `clientFileString` (`+0x2c`): `RACE_ICON_TCOORDS`' and `RACE_INFO_`'s key.
    pub file: String,
    /// The list's third value, 1 or 0 (`0x6b0970`, not traced).
    pub enabled: bool,
    /// `GetFactionForRace` (`0x4e06f0`): the faction group's localized name and its internal
    /// name, `FACTION_BACKDROP_COLOR_TABLE`'s key.
    pub faction: (String, String),
    /// `GetHairCustomization` (`+0x4c`) and `GetFacialHairCustomization` by sex (`+0x44`).
    pub hair: String,
    pub facial_hair: [String; 2],
}

/// One `ChrClasses` row, as `GetAvailableClasses` (`0x4e0830`) reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlueCreateClass {
    pub id: u8,
    pub names: [String; 2],
    /// The row's file name (`+0x1c`): `CLASS_ICON_TCOORDS`' key.
    pub file: String,
    /// The third value, 1 or 0 (`0x6b0970`): App folds in the Death Knight's level-55 gate.
    pub enabled: bool,
    /// `GetSelectedClass`' last three, the role mask of `0x552940` (bits 2, 4 and 8).
    pub roles: [bool; 3],
}

/// What App seeds: the create list, the class rows, the race/class pairs `IsRaceClassValid`
/// allows and the five dial counts per (race, sex).
#[derive(Clone, Debug, Default)]
pub struct GlueCreateCatalog {
    pub races: Vec<GlueCreateRace>,
    pub classes: Vec<GlueCreateClass>,
    pub valid: Vec<(u8, u8)>,
    pub dials: HashMap<(u8, u8), [u8; 5]>,
    /// `GetCreateBackgroundModel`'s `UI_<name>` stage per (race, class).
    pub backgrounds: HashMap<(u8, u8), String>,
    /// `NameGen.dbc`'s names per (race, sex), which `GetRandomName` learns from.
    pub names: HashMap<(u8, u8), Vec<String>>,
}

/// The selection: race and class ids, sex 0 or 1, the dials `[skin, face, hair style, hair
/// colour, facial hair]` and the facing in radians (`0xb6b188`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GlueCreateSelection {
    pub race: u8,
    pub class: u8,
    pub sex: u8,
    pub dials: [u8; 5],
    pub facing: f32,
}

/// The calls App acts on, in call order.
#[derive(Clone, Debug, PartialEq)]
pub enum GlueCreateRequest {
    /// `CreateCharacter(name)` (`0x4e0c60` into `0x4e0380`).
    Create(String),
    /// `SetCharCustomizeFrame(name)` (`0x4e05b0`): the model frame the preview draws in.
    CustomizeFrame(String),
    /// `UpdateCustomizationScene()` (`0x4dfc90` into `0x4f1520`): re-stage the preview.
    UpdateScene,
    /// `CustomizeExistingCharacter(index)` (`0x4e2330`): a paid service starts from a character.
    CustomizeExisting(u32),
}

/// `GetSelectedSex` and `SetSelectedSex` map through `0x9f3cd0`: male 2, female 3.
const SEX_VALUES: [i64; 2] = [2, 3];

/// The API's state, the glue VM's app data (installed with the API).
#[derive(Default)]
pub(crate) struct GlueCreateState {
    catalog: GlueCreateCatalog,
    selection: GlueCreateSelection,
    requests: Vec<GlueCreateRequest>,
    rng: u64,
}

impl GlueCreateState {
    fn race_row(&self) -> Option<&GlueCreateRace> {
        self.catalog
            .races
            .iter()
            .find(|r| r.id == self.selection.race)
    }

    fn race_index(&self) -> Option<usize> {
        self.catalog
            .races
            .iter()
            .position(|r| r.id == self.selection.race)
    }

    fn allows(&self, race: u8, class: u8) -> bool {
        self.catalog.valid.contains(&(race, class))
    }

    fn class_live(&self, class: u8) -> bool {
        self.allows(self.selection.race, class)
            && self
                .catalog
                .classes
                .iter()
                .any(|c| c.id == class && c.enabled)
    }

    fn counts(&self) -> [u8; 5] {
        let s = &self.selection;
        self.catalog
            .dials
            .get(&(s.race, s.sex))
            .copied()
            .unwrap_or([1; 5])
    }

    /// Re-clamp class and dials into the current (race, sex): the class survives when the race
    /// allows it, else the first live row (labelled, as the native screen does).
    fn clamp(&mut self) {
        if !self.class_live(self.selection.class) {
            let first = self
                .catalog
                .classes
                .iter()
                .map(|c| c.id)
                .find(|&c| self.class_live(c));
            self.selection.class = first.unwrap_or(self.selection.class);
        }
        let counts = self.counts();
        for (d, &n) in self.selection.dials.iter_mut().zip(&counts) {
            *d = if n == 0 { 0 } else { (*d).min(n - 1) };
        }
    }

    /// `ResetCharCustomize` (`0x4e1fd0`): the first race, male, its first class, dials 0.
    /// Labelled: the stock comment calls it random; this is the native screen's reset.
    fn reset(&mut self) {
        let race = self.catalog.races.first().map_or(1, |r| r.id);
        self.selection = GlueCreateSelection {
            race,
            class: 0,
            sex: 0,
            dials: [0; 5],
            facing: self.selection.facing,
        };
        self.clamp();
    }

    fn next_random(&mut self) -> u64 {
        let r = &mut self.rng;
        *r ^= *r << 13;
        *r ^= *r >> 7;
        *r ^= *r << 17;
        *r = r.wrapping_add(0x9E37_79B9_7F4A_7C15);
        *r
    }
}

impl super::UiScript {
    /// Install the CharacterCreate glue API on this VM: the glue host's VM only.
    pub fn install_glue_create_api(&self) -> mlua::Result<()> {
        install(self.lua())
    }

    /// Seed the catalog the API answers from, keeping a selection it still allows.
    pub fn set_glue_create_catalog(&mut self, catalog: GlueCreateCatalog) {
        state(self.lua(), |s| {
            s.catalog = catalog;
            s.clamp();
        });
    }

    /// The selection the preview wears.
    pub fn glue_create_selection(&self) -> GlueCreateSelection {
        state(self.lua(), |s| s.selection)
    }

    /// Set the selection from App, e.g. a paid service's character (`0x4e2330`).
    pub fn set_glue_create_selection(&mut self, selection: GlueCreateSelection) {
        state(self.lua(), |s| {
            s.selection = selection;
            s.clamp();
        });
    }

    /// Seed the randomizer, so a session's rolls differ.
    pub fn seed_glue_create_rng(&mut self, seed: u64) {
        state(self.lua(), |s| s.rng = seed | 1);
    }

    /// Drain the create calls App acts on, in call order.
    pub fn take_glue_create_requests(&mut self) -> Vec<GlueCreateRequest> {
        state(self.lua(), |s| std::mem::take(&mut s.requests))
    }
}

/// The state, created with the API; a VM without the API reads a default one.
fn state<R>(lua: &Lua, f: impl FnOnce(&mut GlueCreateState) -> R) -> R {
    if lua.app_data_ref::<GlueCreateState>().is_none() {
        lua.set_app_data(GlueCreateState::default());
    }
    f(&mut lua.app_data_mut::<GlueCreateState>().expect("set above"))
}

mod api;
mod name_gen;
pub(super) use api::install;

#[cfg(test)]
mod tests;
