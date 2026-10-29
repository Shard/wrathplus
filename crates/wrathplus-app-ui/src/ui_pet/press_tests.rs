//! The pet bar fixtures shared by the pet tests: the spells, our pet and player as objects, and a
//! VM that records the pet's list events. The whole-client rig over them (`0x4bd1d0`'s press and
//! the pet's global cooldown) lives in the root layer, `game_plugins::client_tests`.

use wrathplus_formats::SpellDisplay;
use wrathplus_protocol::types::Field;

use crate::app_core::session::ObjectStore;
use crate::net::ClientCommand;

pub const ME: u64 = 0x10;
pub const PET: u64 = 0x2A;
/// `UNIT_FIELD_CHARMEDBY`, `UNIT_FIELD_SUMMONEDBY` and `UNIT_FIELD_CREATEDBY`, low words; the high
/// words stay 0.
pub const CHARMEDBY: u16 = Field::UnitCharmedby.index();
pub const SUMMONEDBY: u16 = Field::UnitSummonedby.index();
pub const CREATEDBY: u16 = Field::UnitCreatedby.index();
pub const FLAGS: u16 = Field::UnitFlags.index();

pub const CLAW: u32 = 16829;
pub const BITE: u32 = 17258;
pub const GROWL: u32 = 14918;

/// Claw's shape: no timer of its own, and the ordinary pet GCD, category 133 for 1500 ms.
pub fn claw() -> SpellDisplay {
    SpellDisplay {
        name: "Claw".into(),
        start_recovery_category: 133,
        start_recovery_ms: 1500,
        ..Default::default()
    }
}

/// Bite's shape: the same GCD pair, and a 10 s category timer of its own.
pub fn bite() -> SpellDisplay {
    SpellDisplay {
        name: "Bite".into(),
        category: 19,
        category_recovery_ms: 10_000,
        ..claw()
    }
}

/// Growl's shape: a 5 s category timer and no GCD pair.
pub fn growl() -> SpellDisplay {
    SpellDisplay {
        name: "Growl".into(),
        category: 82,
        category_recovery_ms: 5_000,
        ..Default::default()
    }
}

/// Our own pet as vmangos fills it, `SUMMONEDBY` and `CREATEDBY` us (`Pet.cpp:264`, `:287`), no
/// flags: the predicate's owner test reads `SUMMONEDBY` (`0x4bd054`).
pub fn owned_pet() -> ObjectStore {
    owned_pet_with(&[])
}

/// [`owned_pet`] with more fields set, which win over its own.
pub fn owned_pet_with(extra: &[(u16, u32)]) -> ObjectStore {
    let mut pairs = vec![
        (SUMMONEDBY, ME as u32),
        (SUMMONEDBY + 1, 0),
        (CREATEDBY, ME as u32),
        (CREATEDBY + 1, 0),
    ];
    pairs.retain(|(index, _)| !extra.iter().any(|(set, _)| set == index));
    pairs.extend_from_slice(extra);
    ObjectStore(wrathplus_protocol::ObjectFields::from_pairs(&pairs))
}

/// Our own player: `OBJECT_FIELD_TYPE` 0x19, alive.
pub fn player() -> ObjectStore {
    player_with(&[])
}

/// [`player`] with more fields set.
pub fn player_with(extra: &[(u16, u32)]) -> ObjectStore {
    let mut pairs = vec![(2, 0x19), (22, 100), (28, 100)];
    pairs.extend_from_slice(extra);
    ObjectStore(wrathplus_protocol::ObjectFields::from_pairs(&pairs))
}

/// A VM whose listener records every event the pet's lists fire into `SEEN`, in order.
pub fn listening_vm() -> wrathplus_ui::script::UiScript {
    let script = wrathplus_ui::script::UiScript::new().expect("a VM");
    listen(&script);
    script
}

/// The listener [`listening_vm`] carries, on a VM that has one already.
pub fn listen(script: &wrathplus_ui::script::UiScript) {
    script
        .run(
            r#"
            SEEN = {}
            local f = CreateFrame("Frame")
            f:RegisterEvent("PET_BAR_UPDATE")
            f:RegisterEvent("PET_BAR_UPDATE_COOLDOWN")
            f:RegisterEvent("ACTIONBAR_UPDATE_COOLDOWN")
            f:RegisterEvent("SPELL_UPDATE_COOLDOWN")
            f:SetScript("OnEvent", function(self, event, arg1, arg2, arg3, arg4, arg5, arg6, arg7, arg8, arg9) table.insert(SEEN, event) end)
        "#,
        )
        .expect("the listener registers");
}

pub fn pet_actions(sent: &[ClientCommand]) -> usize {
    sent.iter()
        .filter(|c| matches!(c, ClientCommand::PetAction { .. }))
        .count()
}
