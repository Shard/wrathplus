//! The UI error line's queues: refusals that world, net and UI code raise and `ui_action`'s
//! drain shows. `ui_action::errors` keeps the resolvers and re-exports these from its old path.

use benilla_ui::messages::MsgKind;
use bevy::prelude::*;

/// Cast failures for the UI error line; the spell id lets the display key on the spell's record.
#[derive(Resource, Default)]
pub(crate) struct CastErrors(pub Vec<CastFail>);

/// One queued cast failure. `arg` fills the message's `%s` (the arm table `0x6e1d8e`); the
/// reference raises its local refusals with none, except the crowd-control `0x8d` (`0x6094f0`),
/// which names the blocking aura's `SpellMechanic.dbc` id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CastFail {
    pub spell_id: u32,
    pub reason: u8,
    pub arg: Option<u32>,
    /// Whose refusal this is, which picks the reference's message table ([`Caster`]).
    pub caster: Caster,
    /// A retry after the item template landed (the `0x78`/`0x5c` cache miss). The reference's
    /// first pass exits past both the line and the log (`0x6e1eab`, `0x6e1efc`); its retry, the
    /// cache callback `0x6e29b0`, calls `DisplayError` directly and never `0x62c360`, so a
    /// redisplay is not logged.
    pub redisplay: bool,
}

/// Who failed to cast, which picks the reference's handler: `SMSG_CAST_RESULT` lands in
/// `0x6e1a00` and `SMSG_PET_CAST_FAILED` in `0x6e8eb0`, each with its own reason-to-errorId map;
/// the rest of `ui_action::cast_fail::cast_fail_text` is shared.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Caster {
    /// Ours: `SMSG_CAST_RESULT` and every local refusal.
    #[default]
    Player,
    /// The pet's or the charm's: `SMSG_PET_CAST_FAILED` only.
    Pet,
}

/// (Dis)mount refusals from `SMSG_MOUNTRESULT`/`SMSG_DISMOUNTRESULT` as `(mount, code)`, resolved
/// by `ui_action::mount_result_key`.
#[derive(Resource, Default)]
pub(crate) struct MountErrors(pub Vec<(bool, u32)>);

/// Taming refusals as the raw `SMSG_PET_TAME_FAILURE` reason. The reference (`0x6e6a20`) resolves
/// the reason's `PETTAME_*` string through the VM (`0x703bf0`) and passes it as `ERR_TAME_FAILED`'s
/// `%s` (`DisplayError(0xee)`), so both lookups wait for the VM at the drain.
#[derive(Resource, Default)]
pub(crate) struct PetTameFailures(pub Vec<u8>);

/// One argText argument; the list is ordered and mixed because a template's specifiers are:
/// `ERR_USE_LOCKED_WITH_SPELL_KNOWN_SI` takes a string then an integer (`0x5f34a9`).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FillArg {
    S(String),
    D(i64),
}

/// One `DisplayError` (`0x496720`) message, whatever its surface: a GlobalStrings key and the
/// argText the reference pushes after the id, at most three strings (the guild event tail
/// `0x5e745f`). Extra arguments are ignored and a starved specifier shows verbatim, as
/// `SStrPrintf` does, so a short list is passed short.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UiError {
    pub key: &'static str,
    pub args: Vec<FillArg>,
}

/// Client-local refusals queued by GlobalStrings key: the `DisplayError` route for errors with no
/// wire code and no spell record.
#[derive(Resource, Default)]
pub(crate) struct UiErrorKeys(pub Vec<UiError>);

/// Lines that arrive already resolved, with no key and no catalog record: they reach the
/// reference's sink `0x4945b0(text, flag)` without `DisplayError`, flag 1 firing
/// `UI_ERROR_MESSAGE` and 0 `UI_INFO_MESSAGE`. `SMSG_NOTIFICATION` (`0x401800`) passes 1,
/// `SMSG_AREA_TRIGGER_MESSAGE` (`0x48f8ff`) passes 0.
#[derive(Resource, Default)]
pub(crate) struct UiErrorTexts(pub Vec<(String, MsgKind)>);
