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
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let (tx, rx) = crossbeam_channel::unbounded();
    app.insert_resource(NetCommands(tx));
    let world = app.world_mut();
    let me = world
        .spawn((
            SelfPlayer,
            Guid(ME),
            ObjectStore(wrathplus_protocol::ObjectFields::from_fields(&[
                (Field::ObjectGuid, ME as u32),
                (Field::ObjectType, 0x19),
                (Field::UnitHealth, 100),
                (Field::UnitMaxHealth, 100),
            ])),
        ))
        .id();
    world.resource_mut::<GuidIndex>().0.insert(ME, me);
    world.resource_mut::<SelfGuid>().0 = Some(ME);
    let display = wrathplus_formats::SpellDisplay {
        active_icon_id: active_icon,
        ..Default::default()
    };
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
    script.run(lua).expect("the script runs");
    world.insert_non_send_resource(script);
    app.world_mut()
        .run_system_once(apply_script_calls)
        .expect("the calls apply as a one-shot system");
    rx.try_iter()
        .filter(|c| {
            matches!(
                c,
                ClientCommand::CastSpell { .. } | ClientCommand::CancelAura { .. }
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
