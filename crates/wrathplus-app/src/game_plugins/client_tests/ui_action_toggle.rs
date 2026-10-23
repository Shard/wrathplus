//! Whole-client tests of `crate::ui_action::toggle`, the press-again cancel: build 12340's cast
//! entry (`0x80da80`) tests the spell with `isActive` (`0x802cb0`) and, when it is lit, cancels it
//! (`0x802f80`, `CMSG_CANCEL_AURA`) instead of casting. They build the client with
//! `headless_client`, so they live in the root layer.

use crate::app_core::session::{ObjectStore, SelfGuid, SelfPlayer};
use crate::net::{ClientCommand, Guid, GuidIndex, NetCommands};
use crate::script_calls::apply_script_calls;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use wrathplus_protocol::types::Field;
use wrathplus_ui::script::{
    ShapeshiftFormView, SpellBookState, SpellSlotView, SpellTabView, UiScript,
};

const ME: u64 = 0x10;

/// Devotion Aura: a stance-bar aura with `ActiveIconID` 122.
const DEVOTION: u32 = 465;

/// `active_icon` is the spell's `ActiveIconID`; `aura` our packet aura of it, `Some(positive)`.
fn press(lua: &str, active_icon: u32, aura: Option<bool>) -> Vec<ClientCommand> {
    let display = wrathplus_formats::SpellDisplay {
        active_icon_id: active_icon,
        ..Default::default()
    };
    press_with(lua, display, aura, &[], &|_| {})
}

/// [`press`] with the spell's whole display and extra `(index, word)` pairs on our descriptor.
fn press_with(
    lua: &str,
    display: wrathplus_formats::SpellDisplay,
    aura: Option<bool>,
    fields: &[(u16, u32)],
    prep: &dyn Fn(&mut World),
) -> Vec<ClientCommand> {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let (tx, rx) = crossbeam_channel::unbounded();
    app.insert_resource(NetCommands(tx));
    let world = app.world_mut();
    let me = world
        .spawn((
            SelfPlayer,
            Guid(ME),
            ObjectStore({
                let mut f = wrathplus_protocol::ObjectFields::from_fields(&[
                    (Field::ObjectGuid, ME as u32),
                    (Field::ObjectType, 0x19),
                    (Field::UnitHealth, 100),
                    (Field::UnitMaxHealth, 100),
                ]);
                f.merge(wrathplus_protocol::ObjectFields::from_pairs(fields));
                f
            }),
        ))
        .id();
    world.resource_mut::<GuidIndex>().0.insert(ME, me);
    world.resource_mut::<SelfGuid>().0 = Some(ME);
    world.insert_resource(crate::app_core::state::Spells {
        catalog: wrathplus_formats::SpellCatalog::from_displays([(DEVOTION, display)].into()),
        ..crate::app_core::state::Spells::empty_for_tests()
    });
    if let Some(positive) = aura {
        crate::net::wrath_auras::set_signed_for_test(world, ME, &[(0, DEVOTION, positive)]);
    }
    let mut script = UiScript::new().expect("a VM");
    script.set_spellbook(SpellBookState {
        tabs: vec![SpellTabView {
            name: "Holy".into(),
            texture: None,
            offset: 0,
            num_spells: 1,
        }],
        slots: vec![SpellSlotView {
            spell_id: DEVOTION,
            name: "Devotion Aura".into(),
            ..Default::default()
        }],
    });
    script.set_shapeshift_forms(vec![ShapeshiftFormView {
        spell_id: DEVOTION,
        name: "Devotion Aura".into(),
        castable: true,
        ..Default::default()
    }]);
    prep(world);
    script.run(lua).expect("the script runs");
    world.insert_non_send_resource(script);
    app.world_mut()
        .run_system_once(apply_script_calls)
        .expect("the calls apply as a one-shot system");
    rx.try_iter()
        .filter(|c| {
            matches!(
                c,
                ClientCommand::CastSpell { .. }
                    | ClientCommand::CancelAura { .. }
                    | ClientCommand::NativeSpellControl(_)
            )
        })
        .collect()
}

fn cast(c: &ClientCommand) -> bool {
    matches!(
        c,
        ClientCommand::CastSpell {
            spell_id: DEVOTION,
            ..
        }
    )
}

fn cancel(c: &ClientCommand) -> bool {
    matches!(c, ClientCommand::CancelAura { spell_id: DEVOTION })
}

/// The stance bar's click (`CastShapeshiftForm`, `0x53c0f0`) and `CastSpellByName` both reach the
/// cast entry: a lit aura cancels; an unlit one, a negative aura or no `ActiveIconID` casts.
#[test]
fn a_lit_aura_re_pressed_cancels_instead_of_casting() {
    for lua in [
        "CastShapeshiftForm(1)",
        r#"CastSpellByName("Devotion Aura")"#,
    ] {
        let sent = press(lua, 122, Some(true));
        assert!(
            sent.len() == 1 && cancel(&sent[0]),
            "{lua} on a lit aura: {sent:?}"
        );
        for (icon, aura, why) in [
            (122, None, "no aura up"),
            (122, Some(false), "the aura is not positive"),
            (0, Some(true), "no ActiveIconID"),
        ] {
            let sent = press(lua, icon, aura);
            assert!(sent.len() == 1 && cast(&sent[0]), "{lua}, {why}: {sent:?}");
        }
    }
}

/// `isActive`'s mount arm: a first effect applying `SPELL_AURA_MOUNTED` passes the gate with no
/// `ActiveIconID`, so re-pressing the mount we ride cancels its aura, which dismounts.
#[test]
fn re_pressing_the_mount_we_ride_cancels_its_aura() {
    let mount = || wrathplus_formats::SpellDisplay {
        effect_apply_aura: [78, 0, 0],
        ..Default::default()
    };
    let lua = r#"CastSpellByName("Devotion Aura")"#;
    let sent = press_with(lua, mount(), Some(true), &[], &|_| {});
    assert!(sent.len() == 1 && cancel(&sent[0]), "mounted: {sent:?}");
    let sent = press_with(lua, mount(), None, &[], &|_| {});
    assert!(
        sent.len() == 1 && cast(&sent[0]),
        "on foot it mounts: {sent:?}"
    );
}

/// `isActive`'s critter arm: a SUMMON effect whose creature is the one in our
/// `UNIT_FIELD_CRITTER` guid is lit, and `0x802f80` dismisses it (`CMSG_DISMISS_CRITTER`).
#[test]
fn re_pressing_our_companion_dismisses_it() {
    const ENTRY: i32 = 7385;
    let critter: u64 = 0xF130_0000_0000_0042 | (ENTRY as u64) << 24;
    let index = Field::UnitCritter.span().unwrap().index;
    let summon = || wrathplus_formats::SpellDisplay {
        effects: [28, 0, 0],
        effect_misc_value: [ENTRY, 0, 0],
        ..Default::default()
    };
    let out = [(index, critter as u32), (index + 1, (critter >> 32) as u32)];
    let lua = r#"CastSpellByName("Devotion Aura")"#;
    let sent = press_with(lua, summon(), None, &out, &|_| {});
    assert!(
        matches!(
            sent[..],
            [ClientCommand::NativeSpellControl(
                wrathplus_protocol::wrath_spell_control::Command::DismissCritter { critter: c }
            )] if c == critter
        ),
        "our companion is out: {sent:?}"
    );
    let sent = press_with(lua, summon(), None, &[], &|_| {});
    assert!(
        sent.len() == 1 && cast(&sent[0]),
        "none out: it summons: {sent:?}"
    );
    // Charmed, `0x802f80` sends nothing, and the lit spell is still not cast.
    let charmed = Field::UnitCharmedby.span().unwrap().index;
    let sent = press_with(
        lua,
        summon(),
        None,
        &[out[0], out[1], (charmed, 0x99)],
        &|_| {},
    );
    assert!(sent.is_empty(), "charmed: {sent:?}");
}

/// The `secureAbilityToggle` guard (`0x80daec`): a lit aura re-pressed within 1500 ms of its own
/// cast request is held, nothing sent and nothing cast; past the window, or with the CVar off, it
/// cancels.
#[test]
fn the_secure_toggle_holds_a_double_press_for_1500_ms() {
    use crate::spell::secure_toggle::RecentCastRequests;
    use std::time::{Duration, Instant};
    let display = || wrathplus_formats::SpellDisplay {
        active_icon_id: 122,
        ..Default::default()
    };
    let requested = |ago_ms: u64, cvar_off: bool| {
        move |world: &mut World| {
            let at = Instant::now() - Duration::from_millis(ago_ms);
            world
                .resource_mut::<RecentCastRequests>()
                .record(DEVOTION, at);
            if cvar_off {
                world.trigger(crate::cvars::CvarChanged {
                    name: "secureAbilityToggle".into(),
                    old: "1".into(),
                    new: "0".into(),
                });
            }
        }
    };
    let lua = r#"CastSpellByName("Devotion Aura")"#;
    let sent = press_with(lua, display(), Some(true), &[], &requested(200, false));
    assert!(
        sent.is_empty(),
        "a double press inside the window: {sent:?}"
    );
    let sent = press_with(lua, display(), Some(true), &[], &requested(1600, false));
    assert!(
        sent.len() == 1 && cancel(&sent[0]),
        "past the window: {sent:?}"
    );
    let sent = press_with(lua, display(), Some(true), &[], &requested(0, true));
    assert!(
        sent.len() == 1 && cancel(&sent[0]),
        "the CVar off: {sent:?}"
    );
}
