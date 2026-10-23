//! The UI error line's queues: refusals that world, net and UI code raise and `ui_action`'s
//! drain shows. `ui_action::errors` keeps the resolvers and re-exports these from its old path.

use wrathplus_ui::messages::MsgKind;
use bevy::prelude::*;

/// Cast failures for the UI error line; the spell id lets the display key on the spell's record.
#[derive(Resource, Default)]
pub struct CastErrors(pub Vec<CastFail>);

impl CastErrors {
    /// Queue a [`CastFail::local`] refusal.
    pub fn push_local(&mut self, spell_id: u32, reason: u8) {
        self.0.push(CastFail::local(spell_id, reason));
    }

    /// Queue a client-local refusal with an argument word, the crowd-control `0x8d`.
    pub fn push_local_arg(&mut self, spell_id: u32, reason: u8, arg: u32) {
        self.0.push(CastFail {
            spell_id,
            reason,
            arg: Some(arg),
            caster: Caster::Player,
            redisplay: false,
        });
    }

    /// Queue the pet's `SMSG_PET_CAST_FAILED` refusal, which carries no argument word
    /// (vmangos `Packets/Pet.cpp:127-132`).
    pub fn push_pet(&mut self, spell_id: u32, reason: u8) {
        self.0.push(CastFail {
            spell_id,
            reason,
            arg: None,
            caster: Caster::Pet,
            redisplay: false,
        });
    }
}

/// One queued cast failure. `arg` fills the message's `%s` (the arm table `0x6e1d8e`); the
/// reference raises its local refusals with none, except the crowd-control `0x8d` (`0x6094f0`),
/// which names the blocking aura's `SpellMechanic.dbc` id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastFail {
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

impl CastFail {
    /// A client-local refusal, with no argument word; always the player's, since the server
    /// decides every pet refusal.
    pub const fn local(spell_id: u32, reason: u8) -> Self {
        Self {
            spell_id,
            reason,
            arg: None,
            caster: Caster::Player,
            redisplay: false,
        }
    }

    /// The same entry, marked as a [`Self::redisplay`].
    pub const fn requeued(self) -> Self {
        Self {
            redisplay: true,
            ..self
        }
    }
}

/// Who failed to cast, which picks the reference's handler: `SMSG_CAST_RESULT` lands in
/// `0x6e1a00` and `SMSG_PET_CAST_FAILED` in `0x6e8eb0`, each with its own reason-to-errorId map;
/// the rest of `ui_action::cast_fail::cast_fail_text` is shared.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Caster {
    /// Ours: `SMSG_CAST_RESULT` and every local refusal.
    #[default]
    Player,
    /// The pet's or the charm's: `SMSG_PET_CAST_FAILED` only.
    Pet,
}

/// (Dis)mount refusals from `SMSG_MOUNTRESULT`/`SMSG_DISMOUNTRESULT` as `(mount, code)`, resolved
/// by `ui_action::mount_result_key`.
#[derive(Resource, Default)]
pub struct MountErrors(pub Vec<(bool, u32)>);

/// Taming refusals as the raw `SMSG_PET_TAME_FAILURE` reason. The reference (`0x6e6a20`) resolves
/// the reason's `PETTAME_*` string through the VM (`0x703bf0`) and passes it as `ERR_TAME_FAILED`'s
/// `%s` (`DisplayError(0xee)`), so both lookups wait for the VM at the drain.
#[derive(Resource, Default)]
pub struct PetTameFailures(pub Vec<u8>);

/// One argText argument; the list is ordered and mixed because a template's specifiers are:
/// `ERR_USE_LOCKED_WITH_SPELL_KNOWN_SI` takes a string then an integer (`0x5f34a9`).
#[derive(Debug, PartialEq, Eq)]
pub enum FillArg {
    S(String),
    D(i64),
}

/// One `DisplayError` (`0x496720`) message, whatever its surface: a GlobalStrings key and the
/// argText the reference pushes after the id, at most three strings (the guild event tail
/// `0x5e745f`). Extra arguments are ignored and a starved specifier shows verbatim, as
/// `SStrPrintf` does, so a short list is passed short.
#[derive(Debug, PartialEq, Eq)]
pub struct UiError {
    pub key: &'static str,
    pub args: Vec<FillArg>,
}

impl UiError {
    /// A message with no arguments.
    pub fn key(key: &'static str) -> Self {
        Self {
            key,
            args: Vec::new(),
        }
    }

    /// One string, the `_S` family.
    pub fn s(key: &'static str, s: impl Into<String>) -> Self {
        Self {
            key,
            args: vec![FillArg::S(s.into())],
        }
    }

    /// Several strings in push order, the `_SS`/`_SSS` family.
    pub fn strings(key: &'static str, args: &[&str]) -> Self {
        Self {
            key,
            args: args.iter().map(|s| FillArg::S((*s).to_string())).collect(),
        }
    }

    /// A mixed list, for a template whose specifiers are not all `%s`.
    pub fn args(key: &'static str, args: Vec<FillArg>) -> Self {
        Self { key, args }
    }

    /// The first string argument.
    #[cfg(any(test, feature = "test-support"))]
    pub fn arg_s(&self) -> Option<&str> {
        self.args.iter().find_map(|a| match a {
            FillArg::S(s) => Some(s.as_str()),
            FillArg::D(_) => None,
        })
    }

    /// The first integer argument.
    #[cfg(any(test, feature = "test-support"))]
    pub fn arg_d(&self) -> Option<i64> {
        self.args.iter().find_map(|a| match a {
            FillArg::D(d) => Some(*d),
            FillArg::S(_) => None,
        })
    }
}

/// Client-local refusals queued by GlobalStrings key: the `DisplayError` route for errors with no
/// wire code and no spell record.
#[derive(Resource, Default)]
pub struct UiErrorKeys(pub Vec<UiError>);

/// Lines that arrive already resolved, with no key and no catalog record: they reach the
/// reference's sink `0x4945b0(text, flag)` without `DisplayError`, flag 1 firing
/// `UI_ERROR_MESSAGE` and 0 `UI_INFO_MESSAGE`. `SMSG_NOTIFICATION` (`0x401800`) passes 1,
/// `SMSG_AREA_TRIGGER_MESSAGE` (`0x48f8ff`) passes 0.
#[derive(Resource, Default)]
pub struct UiErrorTexts(pub Vec<(String, MsgKind)>);

impl UiErrorTexts {
    /// The red `UI_ERROR_MESSAGE` arm (`0x4945b0(text, 1)`).
    pub fn error(&mut self, text: String) {
        self.0.push((text, MsgKind::Error));
    }

    /// The yellow `UI_INFO_MESSAGE` arm (`0x4945b0(text, 0)`).
    pub fn info(&mut self, text: String) {
        self.0.push((text, MsgKind::Info));
    }
}
