//! Critter and mount calls from `script_calls`.

use super::*;

/// `DismissCompanion("CRITTER")` sends `CMSG_DISMISS_CRITTER` with the critter's guid.
#[test]
fn dismiss_critter_sends_the_native_command() {
    let mut f = frame(false);
    apply(
        &mut f,
        vec![ScriptCall::DismissCritter {
            critter: 0xf130_0000_0000_0042,
        }],
    );
    assert!(f.rx.try_iter().any(|c| matches!(
        c,
        ClientCommand::NativeSpellControl(
            wrathplus_protocol::wrath_spell_control::Command::DismissCritter {
                critter: 0xf130_0000_0000_0042
            }
        )
    )));
}

/// `Dismount()` sends `CMSG_CANCEL_MOUNT_AURA` off a flight path; `SetActiveTalentGroup` of the
/// active group or one past the count sends nothing.
#[test]
fn dismount_sends_and_the_active_talent_group_does_not_recast() {
    let group = || wrathplus_protocol::events::TalentGroup {
        talents: Vec::new(),
        glyphs: Vec::new(),
    };
    let mut f = frame(false);
    apply(&mut f, vec![ScriptCall::Dismount]);
    assert!(f.rx.try_iter().any(|c| matches!(
        c,
        ClientCommand::NativeSpellControl(
            wrathplus_protocol::wrath_spell_control::Command::CancelMountAura
        )
    )));
    f.app
        .world_mut()
        .resource_mut::<crate::net::wrath_notices::WrathProgress>()
        .talents
        .insert(false, (0, 0, vec![group(), group()]));
    apply(
        &mut f,
        vec![
            ScriptCall::ActivateTalentGroup { group: 1 },
            ScriptCall::ActivateTalentGroup { group: 3 },
            ScriptCall::ActivateTalentGroup { group: 0 },
        ],
    );
    assert!(casts(&f).is_empty());
}
