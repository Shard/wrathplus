//! The UI error line's queues: refusals that world, net and UI code raise and `ui_action`'s
//! drain shows. `ui_action::errors` keeps the resolvers and re-exports these from its old path.

use bevy::prelude::*;
use wrathplus_ui::messages::MsgKind;

/// Cast failures for the UI error line; the spell id lets the display key on the spell's record.
#[derive(Resource, Default)]
pub struct CastErrors(pub Vec<CastFail>);

impl CastErrors {
    /// Queue a [`CastFail::local`] refusal.
    pub fn push_local(&mut self, spell_id: u32, reason: u8) {
        self.0.push(CastFail::local(spell_id, reason));
    }

    /// Queue a client-local refusal with an argument word, the crowd-control
    /// `PREVENTED_BY_MECHANIC`.
    pub fn push_local_arg(&mut self, spell_id: u32, reason: u8, arg: u32) {
        self.0.push(CastFail {
            spell_id,
            reason,
            arg: Some(arg),
            redisplay: false,
        });
    }
}

/// One queued cast failure of ours, in build 12340's reason numbering
/// (`spell::cast_fail_keys::reason`). `arg` is the wire's first argument word, which fills the
/// message's `%s`; a local refusal has none, except the crowd-control `PREVENTED_BY_MECHANIC`,
/// which names the blocking aura's `SpellMechanic.dbc` id. The pet's refusals are
/// `spell::native_failure`'s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CastFail {
    pub spell_id: u32,
    pub reason: u8,
    pub arg: Option<u32>,
    /// A retry after the item template landed (the `TOTEMS`/`REAGENTS` cache miss): the cache
    /// callback redisplays the line, which is not logged again.
    pub redisplay: bool,
}

impl CastFail {
    /// A client-local refusal, with no argument word.
    pub const fn local(spell_id: u32, reason: u8) -> Self {
        Self {
            spell_id,
            reason,
            arg: None,
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
