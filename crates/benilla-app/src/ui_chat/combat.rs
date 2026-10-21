//! The client-composed lines that outlived the 1.12 combat-log sentences on build 12340: the
//! reputation change (`CHAT_MSG_COMBAT_FACTION_CHANGE`) and the death durability loss
//! (`CHAT_MSG_COMBAT_MISC_INFO`). Each is one GlobalString key, filled as `vsnprintf` does from
//! the install's own `GlobalStrings.lua`; only the slot order is ours, one [`Slot`] list per
//! family. The 1.12 `CHAT_MSG_COMBAT_*`/`CHAT_MSG_SPELL_*` sentence families are gone: 3.3.5
//! prints combat text through `COMBAT_LOG_EVENT` and the stock `Blizzard_CombatLog`, which
//! [`crate::combat_log::native`] feeds. `CombatLogPeriodicSpells` stays, for the floating-text leg.

use bevy::prelude::*;

pub(crate) use crate::app_core::chat::{Family, Fills, PendingCombat, Slot};

/// `CombatLogPeriodicSpells`' change callback.
pub(crate) fn on_cvar(ev: On<crate::cvars::CvarChanged>, mut periodic: ResMut<LogPeriodicSpells>) {
    if ev.is(LOG_PERIODIC_CVAR) {
        periodic.0 = ev.flag();
    }
}

/// `CombatLogPeriodicSpells`. Off, the `SMSG_PERIODICAURALOG` handler `0x626dd0` returns at its
/// first read (`0x626dee`), dropping the floating tick number and the miss word. Read as the CVar's
/// int field (`+0x28`, set at `0x63e127`).
#[derive(Resource, Clone, Copy)]
pub(crate) struct LogPeriodicSpells(pub(crate) bool);

impl Default for LogPeriodicSpells {
    /// The reference's registered default, `"1"`.
    fn default() -> Self {
        Self(true)
    }
}

pub(crate) const LOG_PERIODIC_CVAR: &str = "CombatLogPeriodicSpells";

/// Resolve a family to a finished line; `None`, silently, when the key is absent. A family with no
/// slots prints its template verbatim, so a literal `%` (3.3.5's "a 10% durability loss") is not a
/// conversion; the rest fill as `vsnprintf` over a fixed argument list.
pub(crate) fn compose_line(
    lua: &benilla_ui::script::UiScript,
    family: Family,
    fills: &Fills,
) -> Option<String> {
    let template = global_string(lua, family.key)?;
    if family.slots.is_empty() {
        return Some(template);
    }
    let args: Vec<Arg> = family
        .slots
        .iter()
        .map(|slot| match slot {
            Slot::Subject => Arg::Str(&fills.subject),
            Slot::Named => Arg::Str(&fills.named),
            Slot::Amount => Arg::Num(fills.amount),
        })
        .collect();
    let line = fill(&template, &args);
    if line.is_none() {
        // Our slot order disagrees with the shipped template: a defect, so it warns.
        warn!(
            "chat: {} does not match our slot order ({:?})",
            family.key, family.slots
        );
    }
    line
}

/// One `vsnprintf` argument.
enum Arg<'a> {
    Str(&'a str),
    Num(i64),
}

/// `vsnprintf` over the `%s`/`%d` subset the templates use; `None` on any mismatch.
fn fill(template: &str, args: &[Arg<'_>]) -> Option<String> {
    let mut out = String::with_capacity(template.len() + 32);
    let mut next = args.iter();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('s') => match next.next()? {
                Arg::Str(s) => out.push_str(s),
                Arg::Num(_) => return None,
            },
            Some('d') => match next.next()? {
                Arg::Num(n) => out.push_str(&n.to_string()),
                Arg::Str(_) => return None,
            },
            // Any other conversion is a mismatch, never a half-filled sentence.
            _ => return None,
        }
    }
    next.next().is_none().then_some(out)
}

/// A GlobalString off the VM's globals, `None` when absent or empty: the two tests `0x703bf0`'s
/// callers make (`GetPVPRankInfo` at `0x51aa1c`/`0x51aa20`).
pub(crate) fn global_string(script: &benilla_ui::script::UiScript, key: &str) -> Option<String> {
    script
        .lua()
        .globals()
        .get::<Option<String>>(key)
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
}

/// Declare a family: its GlobalString key, its slots in the order the enUS template consumes them,
/// and that template as the doc.
macro_rules! family {
    ($name:ident, [$($slot:ident),* $(,)?], $doc:literal) => {
        #[doc = $doc]
        pub(crate) const $name: Family = Family {
            key: stringify!($name),
            slots: &[$(Slot::$slot),*],
        };
    };
}

family!(
    DURABILITYDAMAGE_DEATH,
    [],
    "`\"Your equipped items suffer a 10% durability loss.\"` — `SMSG_DURABILITY_DAMAGE_DEATH` \
     carries an empty body, so the line has no arguments, and its `%` is literal."
);
family!(
    FACTION_STANDING_INCREASED,
    [Named, Amount],
    "`\"Your %s reputation has increased by %d.\"` — the name is the faction's."
);
family!(
    FACTION_STANDING_DECREASED,
    [Named, Amount],
    "`\"Your %s reputation has decreased by %d.\"`"
);
family!(
    TRADESKILL_LOG_FIRSTPERSON,
    [Named],
    "`\"You create %s.\"` — the item's name (`0x50bd90`, chat type `0x1e`)."
);
family!(
    TRADESKILL_LOG_THIRDPERSON,
    [Subject, Named],
    "`\"%s creates %s.\"`"
);
family!(
    FEEDPET_LOG_FIRSTPERSON,
    [Named],
    "`\"Your pet begins eating the %s.\"` (`0x50baf0`, chat type `0x1f`)."
);
family!(
    FEEDPET_LOG_THIRDPERSON,
    [Subject, Named],
    "`\"%s's pet begins eating a %s.\"`"
);

/// The CVar rows this feature owns ([`crate::cvars::RegisterCvars`]), each with the
/// reference's register site and its default's standing ([`crate::cvars::Registered`]).
pub(crate) const CVARS: &[crate::cvars::Registered] = {
    use crate::cvars::same;
    &[
        // `CombatLogPeriodicSpells` (`0x6033b3`): the handle is discarded and every use looks it up by
        // name; read as the record's int.
        same(LOG_PERIODIC_CVAR, "1"),
    ]
};

/// Every family this module emits, for the sweep against the shipped templates.
#[cfg(test)]
pub(crate) const ALL_FAMILIES: &[Family] = &[
    DURABILITYDAMAGE_DEATH,
    FACTION_STANDING_INCREASED,
    FACTION_STANDING_DECREASED,
    TRADESKILL_LOG_FIRSTPERSON,
    TRADESKILL_LOG_THIRDPERSON,
    FEEDPET_LOG_FIRSTPERSON,
    FEEDPET_LOG_THIRDPERSON,
];

#[cfg(test)]
mod tests;
