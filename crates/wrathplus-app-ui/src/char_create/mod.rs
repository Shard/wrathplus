//! The character-creation screen: a Bevy-UI overlay laid out like the 1.12 `CharacterCreate.xml`,
//! driving the `"create"` preview booth, with art, captions and sounds read from the player's
//! install. This file holds the selection state, input and the wire.

pub(crate) mod paid;
mod panels;
mod parts;
mod refresh;
mod screen;

use bevy::input::keyboard::KeyboardInput;
use wrathplus_protocol::{CharAction, CharCreateReq};
use wrathplus_ui::widget::EditBoxState;

use crate::textinput::{self, HostClipboard};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::input::ButtonState;
use bevy::prelude::*;

use crate::app_core::state::ClientState;
use crate::app_core::state::Roster;
use crate::entities::CharCreate;
use crate::glue_strings::GlueStrings;
use crate::net::{CharPick, CharRequest};
use crate::portrait::{CreateLook, GlueLook, GluePreview};
use crate::sound::GlueSound;

/// Alliance and Horde race columns, top to bottom: `CharacterCreateEnumerateRaces` fills buttons
/// 1..10 in `GetAvailableRaces()` order (`0x4e1880`) and build 12340's `CharacterCreate.xml`
/// chains 1-5 down the first column, 6-10 down the second, so Alliance then Horde, ascending race
/// id. `RACE_ICON_TCOORDS`'s table order is a name-to-UV lookup and never the layout order.
///
/// The Alliance half is also the race-to-side split `ui_unit::race_faction_group` answers
/// `UnitFactionGroup("player")` with.
pub(crate) const ALLIANCE: [u8; 5] = [1, 3, 4, 7, 11]; // Human, Dwarf, Night Elf, Gnome, Draenei
pub(crate) const HORDE: [u8; 5] = [2, 5, 6, 8, 10]; // Orc, Scourge, Tauren, Troll, Blood Elf
/// The ten class buttons by slot, `GetAvailableClasses()`' `ChrClasses` row order (`0x4e0830`):
/// every class has its button, and one the race cannot be shows disabled
/// (`CharacterCreateEnumerateClasses`' `IsRaceClassValid` arm).
pub(crate) const CLASS_SLOTS: [u8; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 11];
/// The reference's initial facing (`SetCharacterCreateFacing(-15)`), reset on every race switch.
const INITIAL_FACING: f32 = -15.0 * std::f32::consts::PI / 180.0;

/// The character-creation screen and its selection state.
pub struct CharCreatePlugin;

impl Plugin for CharCreatePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CreateSelection>()
            .init_resource::<paid::PaidChange>()
            // Under the stock GlueXML create screen (`ui_script::glue_create`) this one stands down.
            .add_systems(
                OnEnter(ClientState::CharCreate),
                screen::enter_create.run_if(native_create),
            )
            .add_systems(
                OnExit(ClientState::CharCreate),
                screen::exit_create.run_if(native_create),
            )
            .add_systems(Update, debug_enter)
            .add_systems(
                Update,
                (
                    screen::rescale_screen,
                    create_input,
                    debug_auto_create,
                    debug_pick,
                    debug_shot,
                    rotate_model,
                    refresh::refresh_dynamic,
                    refresh_name_box,
                    refresh::refresh_hover,
                    refresh::scroll_info,
                    refresh::scroll_drive,
                    refresh::scroll_visuals,
                    create_result,
                    // The paid services' confirm and status (the one `GlueDialog`).
                    crate::glue::dialog::drive_glue_dialog,
                )
                    .chain()
                    .in_set(crate::glue::GlueScreens::Create)
                    .before(crate::glue::GlueVisuals)
                    .run_if(in_state(ClientState::CharCreate).and(native_create))
                    .after(wrathplus_world::schedule::WorldStage::Net),
            );
    }
}

/// The native screen runs unless the stock GlueXML screen hosts the create (`stock_glue_active`).
fn native_create() -> bool {
    !crate::ui_script::glue::stock_glue_active()
}

/// `WOW_CHARCREATE_NAME=<name>`: a few seconds after the screen is up, fill the name and fire
/// Create once, exercising the whole create path unattended (pair with `WOW_CHARCREATE_SHOT=1`).
fn debug_auto_create(
    mut sel: ResMut<CreateSelection>,
    pick: Res<CharPick>,
    time: Res<Time>,
    mut fired: Local<bool>,
    mut armed_at: Local<Option<f32>>,
) {
    if *fired {
        return;
    }
    let Ok(name) = std::env::var("WOW_CHARCREATE_NAME") else {
        *fired = true;
        return;
    };
    let now = time.elapsed_secs();
    let start = *armed_at.get_or_insert(now);
    if now - start < 6.0 {
        return; // let the model settle and the socket park
    }
    sel.name.set_text(&name);
    sel.creating = true;
    let _ = pick.0.send(CharRequest::Create(sel.request()));
    *fired = true;
    info!("char create: auto-create fired for {:?}", sel.name.text);
}

/// `WOW_CHARCREATE_SHOT=1`: jump to the create screen a few seconds after boot, without a click.
fn debug_enter(
    state: Res<State<ClientState>>,
    mut next: ResMut<NextState<ClientState>>,
    time: Res<Time>,
    mut done: Local<bool>,
) {
    if *done || std::env::var("WOW_CHARCREATE_SHOT").is_err() {
        *done = true;
        return;
    }
    if time.elapsed_secs() > 3.0 && *state.get() == ClientState::CharSelect {
        next.set(ClientState::CharCreate);
        *done = true;
    }
}

/// `WOW_CHARCREATE_PICK="race,sex[,class]"`: applied once the screen is up, after its enter reset;
/// a class the race may not be is ignored.
fn debug_pick(
    state: Res<State<ClientState>>,
    catalog: Option<Res<CharCreate>>,
    mut sel: ResMut<CreateSelection>,
    mut preview: ResMut<GluePreview>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    let Ok(spec) = std::env::var("WOW_CHARCREATE_PICK") else {
        *done = true;
        return;
    };
    if *state.get() != ClientState::CharCreate {
        return;
    }
    let mut it = spec.split(',').map(|s| s.trim().parse::<u8>().ok());
    sel.race = it.next().flatten().unwrap_or(1).max(1);
    sel.sex = it.next().flatten().unwrap_or(0).min(1);
    sel.clamp(catalog.as_deref());
    if let Some(class) = it.next().flatten() {
        if race_classes(catalog.as_deref(), sel.race).contains(&class) {
            sel.class = class;
        } else {
            warn!(
                "char create: pick instrument class {class} invalid for race {} — keeping {}",
                sel.race, sel.class
            );
        }
    }
    // `WOW_CHARCREATE_DIALS="skin,face,hairStyle,hairColor,facialHair"`: each field optional
    // (missing leaves 0), each clamped into the (race, sex)'s range.
    if let Ok(spec) = std::env::var("WOW_CHARCREATE_DIALS") {
        let counts = dial_counts(catalog.as_deref(), sel.race, sel.sex);
        for (i, field) in spec.split(',').take(5).enumerate() {
            let Some(want) = field.trim().parse::<u8>().ok() else {
                continue;
            };
            let n = counts[i].max(1);
            if want >= n {
                warn!(
                    "char create: dial {i} value {want} past the range ({n}) for race {} sex {} — clamped",
                    sel.race, sel.sex
                );
            }
            sel.dials[i] = want.min(n - 1);
        }
    }
    preview.scene = Some(crate::portrait::GlueScene::Race {
        race: sel.race,
        class: sel.class,
    });
    preview.look = Some(GlueLook::Create(sel.look()));
    info!(
        "char create: pick instrument set race {} sex {} class {} dials {:?}",
        sel.race, sel.sex, sel.class, sel.dials
    );
    *done = true;
}

/// `WOW_CHARCREATE_SHOT_OUT=<path>`: once the screen has settled, write one PNG of the window
/// through Bevy's framebuffer readback. Pairs with `WOW_CHARCREATE_SHOT=1`.
fn debug_shot(
    mut commands: Commands,
    time: Res<Time>,
    mut entered_at: Local<Option<f32>>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    let Ok(out) = std::env::var("WOW_CHARCREATE_SHOT_OUT") else {
        *done = true;
        return;
    };
    let start = *entered_at.get_or_insert(time.elapsed_secs());
    if time.elapsed_secs() - start < 5.0 {
        return;
    }
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(out.clone()));
    info!("char create: shot instrument writing {out}");
    *done = true;
}

// ── The selection state ──────────────────────────────────────────────────────────────────────────

/// What the create screen has selected. The dials are `[skin, face, hairStyle, hairColor,
/// facialHair]` indices into [`CharCreate`]'s ranges, re-clamped on every race or gender change;
/// `class` picks the starting outfit the preview wears.
#[derive(Resource, Default)]
pub(crate) struct CreateSelection {
    race: u8,
    sex: u8,
    class: u8,
    dials: [u8; 5],
    /// The typed name; the shared feed enforces letters only and the 12-character cap, pasted too.
    name: EditBoxState,
    /// Waiting on `SMSG_CHAR_CREATE`: Create is disarmed.
    creating: bool,
    /// The roster holds a level-55 character, which the Death Knight button needs
    /// (`DEATHKNIGHT_DISABLED`); set on entry.
    death_knight: bool,
}

/// The five dial counts for a (race, sex), or `[1; 5]` without the catalog.
fn dial_counts(catalog: Option<&CharCreate>, race: u8, sex: u8) -> [u8; 5] {
    catalog
        .and_then(|c| c.0.ranges(race, sex))
        .map(|r| [r.skin, r.face, r.hair_style, r.hair_color, r.facial_hair])
        .unwrap_or([1; 5])
}

impl CreateSelection {
    /// Reset to Human, male, its first class, dials 0.
    fn reset(&mut self, catalog: Option<&CharCreate>) {
        self.race = 1;
        self.sex = 0;
        self.class = catalog
            .and_then(|c| c.0.classes_for_race(1).first().copied())
            .unwrap_or(1);
        self.dials = [0; 5];
        self.name.set_text("");
        self.creating = false;
    }

    /// Re-clamp class and dials into the current (race, sex)'s valid ranges.
    fn clamp(&mut self, catalog: Option<&CharCreate>) {
        if let Some(c) = catalog {
            if !c.0.allows(self.race, self.class) || (self.class == 6 && !self.death_knight) {
                self.class =
                    c.0.classes_for_race(self.race)
                        .first()
                        .copied()
                        .unwrap_or(1);
            }
        }
        let counts = dial_counts(catalog, self.race, self.sex);
        for (d, &n) in self.dials.iter_mut().zip(&counts) {
            *d = if n == 0 { 0 } else { (*d).min(n - 1) };
        }
    }

    /// The booth's look; the class dresses the (race, class, sex) starting outfit.
    fn look(&self) -> CreateLook {
        CreateLook {
            race: self.race,
            sex: self.sex,
            class: self.class,
            skin: self.dials[0],
            face: self.dials[1],
            hair_style: self.dials[2],
            hair_color: self.dials[3],
            facial_hair: self.dials[4],
        }
    }

    fn request(&self) -> CharCreateReq {
        CharCreateReq {
            name: self.name.text.clone(),
            race: self.race,
            class: self.class,
            gender: self.sex,
            skin: self.dials[0],
            face: self.dials[1],
            hair_style: self.dials[2],
            hair_color: self.dials[3],
            facial_hair: self.dials[4],
        }
    }
}

/// One clickable control on the screen.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum CreateAction {
    Race(u8),
    Gender(u8),
    /// A class button by slot ([`CLASS_SLOTS`]); a class the race cannot be is disabled.
    ClassSlot(u8),
    /// Dial index 0..5, direction ±1.
    Dial(u8, i8),
    Randomize,
    Create,
    Back,
    /// Hold to rotate, like the reference's rotate buttons.
    RotateLeft,
    RotateRight,
    /// The model pane: drag to rotate, no click action.
    Model,
}

/// The classes the race may be, ascending class id: CharBaseInfo order, as `GetClassesForRace`
/// enumerates them.
fn race_classes(catalog: Option<&CharCreate>, race: u8) -> Vec<u8> {
    catalog
        .map(|c| c.0.classes_for_race(race))
        .unwrap_or_else(|| CLASS_SLOTS.to_vec())
}

/// `IsRaceClassValid`: the race may be the class.
fn class_allowed(catalog: Option<&CharCreate>, race: u8, class: u8) -> bool {
    race_classes(catalog, race).contains(&class)
}

/// A class button is live: the race may be the class and, for the Death Knight,
/// `DEATHKNIGHT_DISABLED`'s "a level 55 character" holds on this roster. Labelled: native reads
/// that from its own enabled flag (`0x4e0830`'s third value, not traced); AzerothCore enforces the
/// same level server-side. Every class is expansion-enabled on a 3.3.5a realm.
fn class_enabled(catalog: Option<&CharCreate>, sel: &CreateSelection, class: u8) -> bool {
    class_allowed(catalog, sel.race, class) && (class != 6 || sel.death_knight)
}

/// A class id's file string, the `CLASS_ICON_TCOORDS` key.
fn class_file(class: u8) -> &'static str {
    match class {
        1 => "WARRIOR",
        2 => "PALADIN",
        3 => "HUNTER",
        4 => "ROGUE",
        5 => "PRIEST",
        7 => "SHAMAN",
        8 => "MAGE",
        9 => "WARLOCK",
        6 => "DEATHKNIGHT",
        11 => "DRUID",
        _ => "WARRIOR",
    }
}

// ── Input ────────────────────────────────────────────────────────────────────────────────────────

/// Clicks, name typing, Enter and Escape. The name caret blinks at the reference's `blinkSpeed`,
/// 0.5 s (`0x77a790`).
fn create_input(
    buttons: Query<(Entity, &CreateAction)>,
    clicks: Res<crate::glue::GlueClicks>,
    mut keyboard: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    catalog: Option<Res<CharCreate>>,
    mut sel: ResMut<CreateSelection>,
    // The host pasteboard and the window handle its Wayland backend needs, and the layout that
    // names the clipboard chords' letters.
    (mut clipboard, layout): (NonSendMut<HostClipboard>, Res<crate::bindings::LayoutNames>),
    raw_handle: Query<&bevy::window::RawHandleWrapper, With<bevy::window::PrimaryWindow>>,
    time: Res<Time>,
    mut preview: ResMut<GluePreview>,
    pick: Res<CharPick>,
    mut next: ResMut<NextState<ClientState>>,
    mut sounds: MessageWriter<GlueSound>,
    mut rng: Local<u64>,
    // The paid service being made, the dialog its Accept asks through, and the question's text.
    (paid, mut dialog, strings): (
        Res<paid::PaidChange>,
        ResMut<crate::glue::dialog::GlueDialog>,
        Option<Res<GlueStrings>>,
    ),
) {
    // A dialog owns the input while it is up.
    if dialog.is_open() {
        keyboard.clear();
        return;
    }
    let mods = textinput::mods_now(&keys);
    let wl = textinput::wayland_display(raw_handle.iter().next());
    // The create screen has one field and no focus model, so it is always focused while up.
    textinput::tick_caret(&mut sel.name, true, time.delta_secs());
    let cat = catalog.as_deref();
    let mut changed_look = false;
    let mut do_create = false;

    // A Button fires on the release over the button that took the press (`glue::glue_clicks`).
    for (entity, action) in &buttons {
        if !clicks.hit(entity) {
            continue;
        }
        match *action {
            CreateAction::Race(r) => {
                // The reference always clicks; a real switch resets the class to the race's first
                // and the facing to -15°. A race a paid service does not offer is disabled.
                sounds.write(GlueSound("gsCharacterCreationClass"));
                let offered = paid.0.as_ref().is_none_or(|t| {
                    t.race_allowed(r, |race, class| class_allowed(cat, race, class))
                });
                if sel.race != r && offered {
                    sel.race = r;
                    // Labelled: the class survives a switch when the race allows it, else the
                    // race's first (the C++ `SetSelectedRace` is not traced).
                    sel.clamp(cat);
                    preview.yaw = INITIAL_FACING;
                    changed_look = true;
                }
            }
            CreateAction::Gender(g) => {
                sounds.write(GlueSound("gsCharacterCreationClass"));
                if sel.sex != g {
                    sel.sex = g;
                    sel.clamp(cat);
                    changed_look = true;
                }
            }
            CreateAction::ClassSlot(slot) => {
                // `CharacterClass_OnClick` clicks, then switches only to a valid class; a disabled
                // button's `OnClick` does nothing (`self.enable`).
                if let Some(&class) = CLASS_SLOTS.get(slot as usize) {
                    // A paid service keeps the class (`CharacterChangeFixup`).
                    let kept = paid.0.as_ref().is_none_or(|t| t.class == class);
                    if class_enabled(cat, &sel, class) && kept {
                        sounds.write(GlueSound("gsCharacterCreationClass"));
                        if sel.class != class {
                            sel.class = class;
                            changed_look = true;
                        }
                    }
                }
            }
            CreateAction::Dial(dial, dir) => {
                sounds.write(GlueSound("gsCharacterCreationLook"));
                cycle_dial(&mut sel, cat, dial as usize, dir);
                changed_look = true;
            }
            CreateAction::Randomize => {
                sounds.write(GlueSound("gsCharacterCreationLook"));
                randomize(&mut sel, cat, &mut rng);
                changed_look = true;
            }
            CreateAction::Create => do_create = true,
            CreateAction::Back => {
                sounds.write(GlueSound("gsCharacterCreationCancel"));
                next.set(ClientState::CharSelect);
            }
            CreateAction::RotateLeft | CreateAction::RotateRight | CreateAction::Model => {}
        }
    }

    for ev in keyboard.read() {
        if ev.state != ButtonState::Pressed {
            continue;
        }
        if textinput::feed_key(
            &mut sel.name,
            ev,
            mods,
            &layout,
            &mut clipboard,
            wl,
            textinput::CharFilter::Letters,
        ) == textinput::FieldKey::Consumed
        {
            continue;
        }
    }
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
        do_create = true;
    }
    if keys.just_pressed(KeyCode::Escape) {
        sounds.write(GlueSound("gsCharacterCreationCancel"));
        next.set(ClientState::CharSelect);
    }

    if do_create && !sel.creating {
        sounds.write(GlueSound("gsCharacterCreationCreateChar"));
        if paid.0.is_some() {
            // `CharacterCreate_Okay`: a paid service asks first.
            let empty = GlueStrings::default();
            let strings = strings.as_deref().unwrap_or(&empty);
            dialog.open_confirm(strings.text(
                "CONFIRM_PAID_SERVICE",
                "Are you sure you are done changing your character?",
            ));
        } else {
            sel.creating = true;
            let _ = pick.0.send(CharRequest::Create(sel.request()));
        }
    }
    if changed_look {
        preview.scene = Some(crate::portrait::GlueScene::Race {
            race: sel.race,
            class: sel.class,
        });
        preview.look = Some(GlueLook::Create(sel.look()));
    }
}

/// Cycle one dial by `dir`, wrapping within `0..count`.
fn cycle_dial(sel: &mut CreateSelection, catalog: Option<&CharCreate>, dial: usize, dir: i8) {
    let count = dial_counts(catalog, sel.race, sel.sex)[dial].max(1) as i32;
    let cur = sel.dials[dial] as i32;
    sel.dials[dial] = (cur + dir as i32).rem_euclid(count) as u8;
}

/// Set every dial to a random valid index, from a small xorshift.
fn randomize(sel: &mut CreateSelection, catalog: Option<&CharCreate>, rng: &mut u64) {
    let counts = dial_counts(catalog, sel.race, sel.sex);
    for (d, &n) in sel.dials.iter_mut().zip(&counts) {
        *rng ^= *rng << 13;
        *rng ^= *rng >> 7;
        *rng ^= *rng << 17;
        *rng = rng.wrapping_add(0x9E37_79B9_7F4A_7C15);
        *d = if n == 0 { 0 } else { (*rng % n as u64) as u8 };
    }
}

/// Paint the name box through the shared `paint_glue_field`, so it draws like the login boxes.
fn refresh_name_box(
    sel: Res<CreateSelection>,
    mut parts: Query<(
        &crate::glue::widgets::GlueFieldPart,
        Option<&mut Text>,
        &mut Visibility,
    )>,
) {
    crate::glue::widgets::paint_glue_field(&sel.name, true, parts.iter_mut());
}

/// Rotate the preview by dragging the model pane (dragging right increases the facing) or holding
/// a rotate button (left decrements it). The drag uses the select screen's constant: the reference
/// declares `CHARACTER_ROTATION_CONSTANT = 0.6` once in `CharacterSelect.lua` for both screens.
fn rotate_model(
    panes: Query<(&Interaction, &CreateAction)>,
    motion: Res<AccumulatedMouseMotion>,
    time: Res<Time>,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut preview: ResMut<GluePreview>,
) {
    let window = window.single().ok();
    for (interaction, action) in &panes {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match action {
            CreateAction::Model if motion.delta.x != 0.0 => {
                preview.yaw += crate::glue::drag_yaw(motion.delta.x, window);
            }
            CreateAction::RotateLeft => preview.yaw -= crate::glue::ROTATE_RATE * time.delta_secs(),
            CreateAction::RotateRight => {
                preview.yaw += crate::glue::ROTATE_RATE * time.delta_secs()
            }
            _ => {}
        }
    }
}

/// The paid service's dialog answers and reply. Done on `CONFIRM_PAID_SERVICE` runs the request
/// ([`paid::PaidTarget::request`]): a refused name opens the error with its key, else the request
/// goes out under its status. Cancel on the confirm closes it; on the status, closes it
/// (labelled: native `StatusDialogClick` for state 9 is not traced). An applied change (the
/// transport publishes the roster first, `2163ca6b`) returns to select; a refusal replaces the
/// status with its key's dialog.
#[allow(clippy::too_many_arguments)]
fn paid_flow(
    paid: &mut paid::PaidChange,
    dialog: &mut crate::glue::dialog::GlueDialog,
    answers: &mut MessageReader<crate::glue::dialog::GlueDialogAnswer>,
    services: Option<MessageReader<crate::char_select::service_reply::CharServiceReply>>,
    pick: &CharPick,
    sel: &CreateSelection,
    strings: &GlueStrings,
    next: &mut NextState<ClientState>,
) {
    use crate::glue::dialog::DialogKind;
    for answer in answers.read() {
        match answer.kind {
            DialogKind::Confirm if answer.button1 => {
                dialog.close();
                let Some(target) = paid.0.as_ref() else {
                    continue;
                };
                match target.request(&sel.name.text, sel.race, sel.sex, sel.dials) {
                    Err(key) => dialog.open_error(strings.text(key, key)),
                    Ok((command, status)) => {
                        info!(
                            "char create: paid service {:?} for {}",
                            target.service, target.name
                        );
                        let _ = pick.0.send(CharRequest::Service(command));
                        dialog.open_status(strings.text(status, status));
                    }
                }
            }
            DialogKind::Confirm | DialogKind::Status => dialog.close(),
            _ => {}
        }
    }
    // Read whether or not a change is pending: this screen's replies answer it or nothing.
    let replies = crate::char_select::service_reply::replies_for(services, ClientState::CharCreate);
    if paid.0.is_none() {
        return;
    }
    for reply in replies {
        use wrathplus_protocol::wrath_char_services::Reply;
        let code = match &reply {
            _ if reply.changed_roster() => {
                dialog.close();
                paid.0 = None;
                next.set(ClientState::CharSelect);
                continue;
            }
            Reply::Rename { result, .. }
            | Reply::Customize { result, .. }
            | Reply::ChangeRace { result, .. } => *result,
            _ => continue,
        };
        let key = crate::char_select::wrath_char_result_key(code).unwrap_or("RESPONSE_FAILURE");
        info!("char create: paid service refused (code {code:#04x}) — {key}");
        dialog.open_error(strings.text(key, "Character change failed"));
    }
}

// ── Result ───────────────────────────────────────────────────────────────────────────────────────

/// Surface each native create result (`SMSG_CHAR_CREATE`, AzerothCore `ResponseCodes`) in the
/// status line; a success returns to select with the row armed. The transport publishes the
/// refreshed roster before a success (`9bab9156`), and a refusal shows its 12340 status key
/// ([`crate::char_select::wrath_char_result_key`]).
#[allow(clippy::type_complexity)] // Bevy system parameters / a closure table, kept inline
fn create_result(
    native: Option<MessageReader<crate::net::WrathCharActionResultMessage>>,
    mut sel: ResMut<CreateSelection>,
    mut roster: ResMut<Roster>,
    mut next: ResMut<NextState<ClientState>>,
    mut status: Query<&mut Text, With<parts::StatusLine>>,
    strings: Res<GlueStrings>,
    // The paid service: its confirm's answer, the request, and the reply.
    (mut paid, mut dialog, mut answers, services, pick): (
        ResMut<paid::PaidChange>,
        ResMut<crate::glue::dialog::GlueDialog>,
        MessageReader<crate::glue::dialog::GlueDialogAnswer>,
        Option<MessageReader<crate::char_select::service_reply::CharServiceReply>>,
        Res<CharPick>,
    ),
) {
    paid_flow(
        &mut paid,
        &mut dialog,
        &mut answers,
        services,
        &pick,
        &sel,
        &strings,
        &mut next,
    );
    for msg in native
        .into_iter()
        .flat_map(|mut r| r.read().copied().collect::<Vec<_>>())
    {
        if msg.action != CharAction::Create {
            continue;
        }
        sel.creating = false;
        let key =
            crate::char_select::wrath_char_result_key(msg.code).unwrap_or("CHAR_CREATE_FAILED");
        info!("char create: native result {:#04x} — {key}", msg.code);
        if msg.code == wrathplus_protocol::wrath_world::CHAR_CREATE_SUCCESS {
            roster.note_created(sel.name.text.clone());
            next.set(ClientState::CharSelect);
        } else if let Ok(mut text) = status.single_mut() {
            text.0 = strings.text(key, "Character creation failed").to_string();
        }
    }
}

#[cfg(test)]
mod tests;
