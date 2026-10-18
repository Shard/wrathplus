//! Appearance replies into the UI. `SMSG_TITLE_EARNED` prints `NEW_TITLE_EARNED` or
//! `OLD_TITLE_LOST` as a system line, the title formatted with our name and gender
//! (`CharTitles.dbc` through [`crate::quest_rewards::QuestRewardTables`]);
//! `SMSG_ENABLE_BARBER_SHOP` fires `BARBER_SHOP_OPEN`; `SMSG_BARBER_SHOP_RESULT` 0 fires
//! `BARBER_SHOP_SUCCESS` then `BARBER_SHOP_APPEARANCE_APPLIED`, 1 shows `ERR_NOT_ENOUGH_MONEY`,
//! and 2 (refused) shows nothing; getting up from the chair fires `BARBER_SHOP_CLOSE`. Labelled:
//! the barber events' order and the money line are untraced in the client.
//!
//! While the chair is open, [`feed_barber`] publishes the shop (`set_wrath_barber_shop`): the
//! `BarberShopStyle.dbc` rows for our race and sex, `gtBarberShopCostBase` at our level, the
//! race's hair colors and customization tokens, and `PLAYER_BYTES`. `ApplyBarberShopStyle` sends
//! `CMSG_ALTER_APPEARANCE`, and the preview dresses our own body
//! ([`crate::entities::BarberPreview`]). Labelled: `CancelBarberShop` (`0x52e9f0`, untraced) stands
//! the player up here, which closes the window as getting up does.
use super::{NetHandlerApp, ObjectStore, SelfGuid, SelfPlayer};
use benilla_protocol::wrath_appearance::Packet;
use benilla_protocol::{SessionEvent, SessionEventKind};
use benilla_ui::script::{ScriptDialect, UiScript};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct AppearanceReceipts(VecDeque<Packet>);

/// The barber window's lifetime: opened by `SMSG_ENABLE_BARBER_SHOP`, closed when the player gets
/// up from the chair (`BARBER_SHOP_CLOSE`).
#[derive(Resource, Default)]
pub(crate) struct BarberChair {
    open: bool,
    /// The player has been seen in a chair stand state since the open.
    seated: bool,
}

/// `UNIT_STAND_STATE_SIT_LOW_CHAIR`..`SIT_HIGH_CHAIR`: the barber chair sits the player at
/// `SIT_LOW_CHAIR + chairheight` (AC `GameObject::Use`).
fn in_chair(stand_state: u8) -> bool {
    (4..=6).contains(&stand_state)
}

/// The chair's next state for a stand-state reading; true when the window closes.
fn chair_step(chair: &mut BarberChair, stand_state: Option<u8>) -> bool {
    if !chair.open {
        return false;
    }
    match stand_state {
        Some(s) if in_chair(s) => {
            chair.seated = true;
            false
        }
        Some(_) if chair.seated => {
            *chair = BarberChair::default();
            true
        }
        _ => false,
    }
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<AppearanceReceipts>()
        .init_resource::<BarberChair>()
        .net_handler(SessionEventKind::WrathAppearance, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear)
        .init_resource::<crate::entities::BarberPreview>()
        .add_systems(Startup, load_barber.after(benilla_assets::AssetSet::Open))
        .add_systems(
            Update,
            (
                feed_appearance,
                feed_titles,
                feed_barber.after(feed_appearance),
            )
                .in_set(crate::ui_script::UiFeed)
                .run_if(crate::ui_script::ingame_ui_up),
        );
}

/// The barber shop's tables; either is absent when the install lacks it.
#[derive(Resource, Default)]
pub(crate) struct BarberTables {
    styles: Option<benilla_formats::BarberStyles>,
    cost: Option<benilla_formats::BarberCostBase>,
}

fn load_barber(mut commands: Commands, assets: Option<Res<benilla_assets::WorldAssets>>) {
    use benilla_assets::LockRecover;
    let Some(assets) = assets else { return };
    let mut chain = assets.chain.lock_recover();
    commands.insert_resource(BarberTables {
        styles: benilla_formats::load_barber_styles(&mut chain).ok(),
        cost: benilla_formats::load_barber_cost_base(&mut chain).ok(),
    });
}

/// Who sits in the chair: race, sex, level and `[skin, hair style, hair color, facial hair]`.
struct Sitter {
    race: u8,
    sex: u8,
    level: u32,
    look: [u8; 4],
}

/// The shop for a sitter. Hair and facial-hair rows are always offered; the skin rows exist only
/// for a race the barber can reskin, so an empty list is `CanAlterSkin`'s false.
fn barber_shop(
    tables: &BarberTables,
    sitter: &Sitter,
    hair_colors: u32,
    tokens: (&str, &str),
) -> benilla_ui::script::WrathBarberShop {
    use benilla_formats::barber_kind;
    let rows = |kind| -> Vec<benilla_ui::script::WrathBarberStyle> {
        tables.styles.as_ref().map_or_else(Vec::new, |styles| {
            styles
                .for_look(kind, u32::from(sitter.race), u32::from(sitter.sex))
                .map(|s| benilla_ui::script::WrathBarberStyle {
                    id: s.id,
                    name: s.name.clone(),
                    description: s.description.clone(),
                    cost_modifier: s.cost_modifier,
                    data: s.data,
                })
                .collect()
        })
    };
    let [skin, hair_style, hair_color, facial_hair] = sitter.look.map(u32::from);
    benilla_ui::script::WrathBarberShop {
        base_cost: tables
            .cost
            .as_ref()
            .and_then(|c| c.at_level(sitter.level))
            .unwrap_or(0.0),
        hair_styles: rows(barber_kind::HAIR),
        facial_styles: rows(barber_kind::FACIAL_HAIR),
        skin_styles: rows(barber_kind::SKIN),
        hair_colors,
        hair_style,
        hair_color,
        facial_hair,
        skin,
        hair_customization: tokens.0.to_string(),
        facial_customization: tokens.1.to_string(),
    }
}

/// The barber verb's packet: `Apply` sends `CMSG_ALTER_APPEARANCE`; `Cancel` stands up.
fn barber_command(request: benilla_ui::script::WrathBarberRequest) -> super::ClientCommand {
    use benilla_protocol::wrath_appearance::Command;
    use benilla_ui::script::WrathBarberRequest;
    match request {
        WrathBarberRequest::Apply {
            hair,
            hair_color,
            facial_hair,
            skin_color,
        } => super::ClientCommand::NativeAppearance(Command::AlterAppearance {
            hair,
            hair_color,
            facial_hair,
            skin_color,
        }),
        WrathBarberRequest::Cancel => super::ClientCommand::StandStateChange { state: 0 },
    }
}

#[allow(clippy::too_many_arguments)]
fn feed_barber(
    script: Option<NonSendMut<UiScript>>,
    chair: Res<BarberChair>,
    tables: Option<Res<BarberTables>>,
    races: Option<Res<crate::entities::CharCreate>>,
    self_q: Query<&ObjectStore, With<SelfPlayer>>,
    commands: Res<super::NetCommands>,
    mut preview: ResMut<crate::entities::BarberPreview>,
    mut last: Local<crate::ui_script::VmMemo<Option<Option<benilla_ui::script::WrathBarberShop>>>>,
) {
    let Some(mut script) = script else {
        return;
    };
    if script.dialect() != ScriptDialect::Wrath12340 {
        return;
    }
    let sitter = self_q.iter().next().and_then(|s| {
        let s = &s.0;
        Some(Sitter {
            race: s.unit_race()?,
            sex: s.unit_gender()?,
            level: s.unit_level().unwrap_or(1),
            look: [
                s.player_skin().unwrap_or(0),
                s.player_hair_style().unwrap_or(0),
                s.player_hair_color().unwrap_or(0),
                s.player_facial_hair().unwrap_or(0),
            ],
        })
    });
    let shop = chair
        .open
        .then_some(())
        .and(tables.as_deref())
        .zip(sitter.as_ref())
        .map(|(tables, sitter)| {
            let races = races.as_deref().map(|r| &r.0);
            let hair_colors = races
                .and_then(|r| r.ranges(sitter.race, sitter.sex))
                .map_or(0, |r| u32::from(r.hair_color));
            let hair = races
                .and_then(|r| r.hair_customization(sitter.race))
                .unwrap_or_default();
            let facial = races
                .and_then(|r| r.facial_hair_customization(sitter.race, sitter.sex))
                .unwrap_or_default();
            barber_shop(tables, sitter, hair_colors, (hair, facial))
        });
    let (last, reset) = last.get_reset(&script);
    if reset || last.as_ref() != Some(&shop) {
        *last = Some(shop.clone());
        script.set_wrath_barber_shop(shop);
    }
    for request in script.take_wrath_barber_requests() {
        let _ = commands.0.send(barber_command(request));
    }
    let shown = script
        .wrath_barber_preview()
        .map(|(hair_style, hair_color, facial_hair, skin)| {
            [skin, hair_style, hair_color, facial_hair].map(|v| u8::try_from(v).unwrap_or(0))
        });
    if preview.0 != shown {
        preview.0 = shown;
    }
}

fn receive(In(event): In<SessionEvent>, mut receipts: ResMut<AppearanceReceipts>) {
    if let SessionEvent::WrathAppearance { packet } = event {
        receipts.0.push_back(packet);
    }
}

fn clear(
    In(event): In<SessionEvent>,
    mut receipts: ResMut<AppearanceReceipts>,
    mut chair: ResMut<BarberChair>,
) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        receipts.0.clear();
        *chair = BarberChair::default();
    }
}

#[allow(clippy::too_many_arguments)]
fn feed_appearance(
    script: Option<NonSendMut<UiScript>>,
    mut receipts: ResMut<AppearanceReceipts>,
    tables: Option<Res<crate::quest_rewards::QuestRewardTables>>,
    names: Res<crate::names::NameCache>,
    commands: Res<super::NetCommands>,
    self_guid: Res<SelfGuid>,
    self_q: Query<&ObjectStore, With<SelfPlayer>>,
    mut chat: ResMut<crate::ui_chat::ChatLog>,
    mut errors: ResMut<crate::ui_action::UiErrorKeys>,
    mut chair: ResMut<BarberChair>,
) {
    let Some(mut script) = script else {
        return;
    };
    if script.dialect() != ScriptDialect::Wrath12340 {
        return;
    }
    let stand_state = self_q.iter().next().map(|s| s.0.unit_stand_state());
    if chair_step(&mut chair, stand_state) {
        script.queue_event("BARBER_SHOP_CLOSE", Vec::new());
    }
    if receipts.0.is_empty() {
        return;
    }
    let subject = crate::npc_text::Subject {
        name: self_guid
            .0
            .and_then(|g| names.resolve(g, &commands))
            .unwrap_or_default()
            .to_string(),
        race: 0,
        class: 0,
        gender: self_q
            .iter()
            .next()
            .and_then(|s| s.0.unit_gender())
            .unwrap_or(0),
    };
    for packet in receipts.0.drain(..) {
        match packet {
            Packet::TitleEarned { bit_index, earned } => {
                let key = if earned {
                    "NEW_TITLE_EARNED"
                } else {
                    "OLD_TITLE_LOST"
                };
                let (Some(template), Some(title)) = (
                    crate::ui_action::global_string(&script, key),
                    tables
                        .as_ref()
                        .and_then(|t| t.title_by_bit(bit_index, &subject)),
                ) else {
                    continue;
                };
                crate::ui_chat::push_system_line(&mut chat, template.replacen("%s", &title, 1));
            }
            Packet::EnableBarberShop => {
                *chair = BarberChair {
                    open: true,
                    seated: false,
                };
                script.queue_event("BARBER_SHOP_OPEN", Vec::new());
            }
            Packet::BarberShopResult { result: 0 } => {
                script.queue_event("BARBER_SHOP_SUCCESS", Vec::new());
                script.queue_event("BARBER_SHOP_APPEARANCE_APPLIED", Vec::new());
            }
            Packet::BarberShopResult { result: 1 } => errors
                .0
                .push(crate::ui_action::UiError::key("ERR_NOT_ENOUGH_MONEY")),
            Packet::BarberShopResult { .. } => {}
        }
    }
}

/// The player's titles for Native UI's title getters: the `CharTitles.dbc` catalog once per VM,
/// and `PLAYER__FIELD_KNOWN_TITLES` (three 64-bit words) with `PLAYER_CHOSEN_TITLE` when they move.
fn feed_titles(
    script: Option<NonSendMut<UiScript>>,
    tables: Option<Res<crate::quest_rewards::QuestRewardTables>>,
    self_q: Query<&ObjectStore, With<SelfPlayer>>,
    mut catalog_sent: Local<crate::ui_script::VmMemo<bool>>,
    mut last: Local<crate::ui_script::VmMemo<Option<(Vec<u64>, u32)>>>,
) {
    use benilla_protocol::messages::Field;
    let Some(mut script) = script else {
        return;
    };
    if script.dialect() != ScriptDialect::Wrath12340 {
        return;
    }
    let (sent, _) = catalog_sent.get_reset(&script);
    if !*sent {
        if let Some(titles) = tables.as_ref().and_then(|t| t.char_titles()) {
            *sent = true;
            script.set_wrath_char_titles(std::sync::Arc::new(
                titles
                    .iter()
                    .map(|t| benilla_ui::script::WrathCharTitle {
                        id: t.id,
                        male: t.male_name.clone(),
                        female: t.female_name.clone(),
                        mask_id: t.mask_id,
                    })
                    .collect(),
            ));
        }
    }
    let Some(store) = self_q.iter().next() else {
        return;
    };
    let known: Vec<u64> = [
        Field::PlayerKnownTitles,
        Field::PlayerKnownTitles1,
        Field::PlayerKnownTitles2,
    ]
    .into_iter()
    .map(|f| store.0.get_field_guid(f, 0).unwrap_or(0))
    .collect();
    let chosen = store.0.get(Field::PlayerChosenTitle).unwrap_or(0);
    let now = Some((known, chosen));
    let (last, reset) = last.get_reset(&script);
    if reset || *last != now {
        *last = now.clone();
        let (known, chosen) = now.expect("just built");
        script.set_wrath_player_titles(known, chosen);
    }
}

#[cfg(test)]
#[path = "wrath_appearance_tests.rs"]
mod tests;
