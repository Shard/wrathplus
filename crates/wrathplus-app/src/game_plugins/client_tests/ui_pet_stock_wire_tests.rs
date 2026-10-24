//! The stock 3.3.5 pet bar onto the wire, whole client headless: each `PetActionButtonN` click on a
//! warlock-shaped native bar (`SMSG_PET_SPELLS`) queues the `CMSG_PET_ACTION` bytes AzerothCore's
//! `HandlePetAction` reads (`PetHandler.cpp:57-67`: pet guid, the slot's word, whose low 24 bits
//! are the action and high 8 its type, then the target guid), and a right-click on a spell flips
//! its autocast through `CMSG_PET_SET_ACTION` (`HandlePetSetAction`, `PetHandler.cpp:696`). The
//! bytes are the writer's own ([`crate::net::io::spell_control_packet`]).
//!
//! An Imp whose Firebolt arrives with autocast off (`ACT_DISABLED`, AzerothCore's default for a
//! new pet spell, `Pet.cpp:1810`) cannot fight: `PetAI::_canMeleeAttack` (`PetAI.cpp:100`) lets it
//! attack only with its bolt on autocast, which only this right-click (or `CMSG_PET_SPELL_AUTOCAST`)
//! turns on.

use super::ui_pet_press_tests::Rig;
use super::ui_pet_usable::{scene, settle, with_stock_ui};
use crate::app_core::state::PetBar;
use crate::net::ClientCommand;
use crate::ui_pet::press_tests::{CLAW, PET};
use bevy::ecs::system::RunSystemOnce;
use wrathplus_protocol::wrath_pet::{Action, PetSpells};

/// The foe [`scene`] selects.
const FOE: u64 = 0x77;
const CMSG_PET_ACTION: u32 = 0x0175;
const CMSG_PET_SET_ACTION: u32 = 0x0174;

/// AzerothCore's `CharmInfo::BuildActionBar` words for a warlock's pet (`ActiveStates` in the high
/// byte: `ACT_COMMAND` 7, `ACT_REACTION` 6, `ACT_DISABLED` 0x81 = autocastable, autocast off).
const ATTACK: u32 = 0x0700_0002;
const FOLLOW: u32 = 0x0700_0001;
const STAY: u32 = 0x0700_0000;
const BOLT_OFF: u32 = 0x8100_0000 | CLAW;
const BOLT_ON: u32 = 0xC100_0000 | CLAW;
const AGGRESSIVE: u32 = 0x0600_0002;
const DEFENSIVE: u32 = 0x0600_0001;
const PASSIVE: u32 = 0x0600_0000;

/// The bar `SMSG_PET_SPELLS` gave the playtest's Imp: commands, the bolt, three empties, reactions.
fn warlock_bar() -> PetSpells {
    let words = [
        ATTACK,
        FOLLOW,
        STAY,
        BOLT_OFF,
        0x0100_0000,
        0x0100_0000,
        0x0100_0000,
        AGGRESSIVE,
        DEFENSIVE,
        PASSIVE,
    ];
    PetSpells {
        guid: PET,
        command: 1,
        actions: words.map(Action),
        spells: vec![Action(BOLT_OFF)],
        ..Default::default()
    }
}

/// A click on `PetActionButton<slot>` at its own centre, the frame's script calls and the bar's
/// other intents applied, and what the click sent.
fn click(rig: &mut Rig, slot: u32, button: &str) -> Vec<ClientCommand> {
    let world = rig.app.world_mut();
    let (x, y): (f32, f32) = world
        .non_send_resource_mut::<wrathplus_ui::script::UiScript>()
        .eval(&format!("return PetActionButton{slot}:GetCenter()"))
        .unwrap();
    {
        let mut script = world.non_send_resource_mut::<wrathplus_ui::script::UiScript>();
        script.mouse_button(x, y, button, true);
        script.mouse_button(x, y, button, false);
    }
    world
        .run_system_once(crate::script_calls::apply_script_calls)
        .expect("the frame's script calls apply");
    world
        .run_system_once(crate::ui_pet::drain::drain_pet_actions)
        .expect("the bar's other intents apply");
    rig.sent()
}

/// The one packet a click sent, as the writer frames it.
fn wire(sent: Vec<ClientCommand>) -> (u32, Vec<u8>) {
    let [command] = <[ClientCommand; 1]>::try_from(sent).expect("exactly one command");
    crate::net::io::spell_control_packet(command).expect("a spell-control command")
}

fn pet_action(word: u32, target: u64) -> (u32, Vec<u8>) {
    let body = [
        &PET.to_le_bytes()[..],
        &word.to_le_bytes(),
        &target.to_le_bytes(),
    ]
    .concat();
    (CMSG_PET_ACTION, body)
}

#[test]
fn the_stock_pet_bar_sends_each_order_as_azerothcore_reads_it() {
    wrathplus_formats::wow_data_or_skip!();
    let mut rig = scene();
    rig.app.world_mut().resource_mut::<PetBar>().native = Some(warlock_bar());
    with_stock_ui(&mut rig);
    settle(&mut rig);

    // Attack goes at the selection; the other orders carry it too, and the server ignores it.
    assert_eq!(
        wire(click(&mut rig, 1, "LeftButton")),
        pet_action(ATTACK, FOE),
        "Attack"
    );
    assert_eq!(
        wire(click(&mut rig, 2, "LeftButton")),
        pet_action(FOLLOW, FOE),
        "Follow"
    );
    assert_eq!(
        wire(click(&mut rig, 3, "LeftButton")),
        pet_action(STAY, FOE),
        "Stay"
    );
    for (slot, word, mode) in [
        (8, AGGRESSIVE, "Aggressive"),
        (9, DEFENSIVE, "Defensive"),
        (10, PASSIVE, "Passive"),
    ] {
        settle(&mut rig);
        assert_eq!(
            wire(click(&mut rig, slot, "LeftButton")),
            pet_action(word, FOE),
            "{mode}"
        );
    }

    // A right-click on the bolt turns its autocast on: slot 4 is wire position 3.
    settle(&mut rig);
    let on = [
        &PET.to_le_bytes()[..],
        &3u32.to_le_bytes(),
        &BOLT_ON.to_le_bytes(),
    ]
    .concat();
    assert_eq!(
        wire(click(&mut rig, 4, "RightButton")),
        (CMSG_PET_SET_ACTION, on),
        "autocast on"
    );
}
