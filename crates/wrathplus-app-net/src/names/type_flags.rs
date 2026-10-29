//! The creature template's `type_flags` dword from `SMSG_CREATURE_QUERY_RESPONSE`, cached by the
//! reference at `[CGUnit+0xb30] + 0x14`. Adjacent bits mean unrelated things, and the reference
//! gives each its own getter, so every consumer names the bit it reads:
//!
//! | bit | reference getter | meaning |
//! |---|---|---|
//! | `0x1` | `0x529d15` | TAMEABLE |
//! | `0x2` | `0x605f70` | VISIBLE_TO_GHOSTS |
//! | `0x4` | `0x612530` | BOSS_MOB |
//! | `0x8` | `0x6125f0` | [`DO_NOT_PLAY_WOUND_ANIM`] |
//! | `0x10` | `0x612610` (inverted) | [`NO_FACTION_TOOLTIP`] |
//! | `0x20` | `0x623b70` | [`MORE_AUDIBLE`] |
//! | `0x40` | `0x60d840` | SPELL_ATTACKABLE / no harmful vertex colouring |
//! | `0x80` | `0x613230` = `CanInteractWhileDead` | INTERACT_WHILE_DEAD |
//!
//! The names of `0x1`-`0x40` follow vmangos `CreatureDefines.h:146-152`.

/// `0x8`: the reference skips the victim wound flinch for it (`0x60ea9f` in `0x60ea70`); the
/// blood spurt and the floating combat text still play.
pub const DO_NOT_PLAY_WOUND_ANIM: u32 = 0x8;

/// `0x10`: the unit tooltip drops its faction line (`0x612610`, which returns it inverted).
pub const NO_FACTION_TOOLTIP: u32 = 0x10;

/// `0x20`: the pass-2 election keeps an off-screen creature ticking so its combat stays
/// audible (`0x607da0`'s `0x623b70` arm).
pub const MORE_AUDIBLE: u32 = 0x20;
