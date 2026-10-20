//! The pet bar fixtures shared by the pet tests: the spells, our pet and player as objects, and a
//! VM that records the pet's list events. The whole-client rig over them (`0x4bd1d0`'s press and
//! the pet's global cooldown) lives in the root layer, `game_plugins::client_tests`.

use benilla_formats::SpellDisplay;
use benilla_protocol::messages::{Field, FieldLayout};

use crate::net::{ClientCommand, ObjectStore};

pub(crate) const ME: u64 = 0x10;
pub(crate) const PET: u64 = 0x2A;
/// `UNIT_FIELD_CHARMEDBY`, `UNIT_FIELD_SUMMONEDBY` and `UNIT_FIELD_CREATEDBY`, low words; the high
/// words stay 0.
pub(crate) const CHARMEDBY: u16 = FieldLayout::Vanilla5875.index_of(Field::UnitCharmedby);
pub(crate) const SUMMONEDBY: u16 = FieldLayout::Vanilla5875.index_of(Field::UnitSummonedby);
pub(crate) const CREATEDBY: u16 = FieldLayout::Vanilla5875.index_of(Field::UnitCreatedby);
pub(crate) const FLAGS: u16 = FieldLayout::Vanilla5875.index_of(Field::UnitFlags);
/// `UNIT_FIELD_AURA` slot 0 and `UNIT_FIELD_AURAFLAGS`, a nibble per slot.
pub(crate) const AURA: u16 = FieldLayout::Vanilla5875.index_of(Field::UnitAura);
pub(crate) const AURAFLAGS: u16 = FieldLayout::Vanilla5875.index_of(Field::UnitAuraFlags);

pub(crate) const CLAW: u32 = 16829;
pub(crate) const BITE: u32 = 17258;
pub(crate) const GROWL: u32 = 14918;
pub(crate) const COWER: u32 = 1742;

/// Claw's shape: no timer of its own, and the ordinary pet GCD, category 133 for 1500 ms.
pub(crate) fn claw() -> SpellDisplay {
    SpellDisplay {
        name: "Claw".into(),
        start_recovery_category: 133,
        start_recovery_ms: 1500,
        ..Default::default()
    }
}

/// Bite's shape: the same GCD pair, and a 10 s category timer of its own.
pub(crate) fn bite() -> SpellDisplay {
    SpellDisplay {
        name: "Bite".into(),
        category: 19,
        category_recovery_ms: 10_000,
        ..claw()
    }
}

/// Growl's shape: a 5 s category timer and no GCD pair.
pub(crate) fn growl() -> SpellDisplay {
    SpellDisplay {
        name: "Growl".into(),
        category: 82,
        category_recovery_ms: 5_000,
        ..Default::default()
    }
}

/// Cower's shape here: a toggle with an active icon, so a press with its aura up cancels it.
pub(crate) fn cower() -> SpellDisplay {
    SpellDisplay {
        name: "Cower".into(),
        active_icon_id: 122,
        ..claw()
    }
}

/// Our own pet as vmangos fills it, `SUMMONEDBY` and `CREATEDBY` us (`Pet.cpp:264`, `:287`), no
/// flags: the predicate's owner test reads `SUMMONEDBY` (`0x4bd054`).
pub(crate) fn owned_pet() -> ObjectStore {
    owned_pet_with(&[])
}

/// [`owned_pet`] with more fields set, which win over its own.
pub(crate) fn owned_pet_with(extra: &[(u16, u32)]) -> ObjectStore {
    let mut pairs = vec![
        (SUMMONEDBY, ME as u32),
        (SUMMONEDBY + 1, 0),
        (CREATEDBY, ME as u32),
        (CREATEDBY + 1, 0),
    ];
    pairs.retain(|(index, _)| !extra.iter().any(|(set, _)| set == index));
    pairs.extend_from_slice(extra);
    ObjectStore(benilla_protocol::ObjectFields::from_pairs(&pairs))
}

/// Our own player: `OBJECT_FIELD_TYPE` 0x19, alive.
pub(crate) fn player() -> ObjectStore {
    player_with(&[])
}

/// [`player`] with more fields set.
pub(crate) fn player_with(extra: &[(u16, u32)]) -> ObjectStore {
    let mut pairs = vec![(2, 0x19), (22, 100), (28, 100)];
    pairs.extend_from_slice(extra);
    ObjectStore(benilla_protocol::ObjectFields::from_pairs(&pairs))
}

/// A VM whose listener records every event the pet's lists fire into `SEEN`, in order.
pub(crate) fn listening_vm() -> benilla_ui::script::UiScript {
    let script = benilla_ui::script::UiScript::new().expect("a VM");
    listen(&script);
    script
}

/// The listener [`listening_vm`] carries, on a VM that has one already.
pub(crate) fn listen(script: &benilla_ui::script::UiScript) {
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

pub(crate) fn pet_actions(sent: &[ClientCommand]) -> usize {
    sent.iter()
        .filter(|c| matches!(c, ClientCommand::PetAction { .. }))
        .count()
}
