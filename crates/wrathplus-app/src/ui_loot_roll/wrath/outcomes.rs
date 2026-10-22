//! Native outcome chat, using build-12340 GlobalStrings.lua LOOT_ROLL_* templates.
use crate::{
    items::Items,
    names::NameCache,
    net::NetCommands,
    ui_chat::{ChatEvent, ChatEventKind, ChatLog},
};
use wrathplus_protocol::wrath_roll::Roll;
use wrathplus_ui::strings::Arg;

/// A `GlobalStrings.lua` lookup, the VM's.
type Strings<'a> = &'a dyn Fn(&str) -> Option<String>;

/// The vote's key stem: the selection and roll keys spell Disenchant `DISENCHANT` and `DE`.
fn kind(vote: u8) -> Option<(&'static str, &'static str)> {
    match vote {
        1 => Some(("NEED", "NEED")),
        2 => Some(("GREED", "GREED")),
        3 => Some(("DISENCHANT", "DE")),
        _ => None,
    }
}

/// One outcome's line, its `LOOT_ROLL_*` template off `get` (the VM's `GlobalStrings.lua`).
fn line(
    roll: &Roll,
    name: &str,
    is_self: bool,
    link: &str,
    detailed: bool,
    female: Option<bool>,
    get: Strings<'_>,
) -> Option<String> {
    let fill = |key: &str, args: &[Arg<'_>]| Some(wrathplus_ui::strings::fill(&get(key)?, args));
    match roll {
        Roll::AllPassed { .. } => fill("LOOT_ROLL_ALL_PASSED", &[Arg::S(link)]),
        Roll::Won {
            number, kind: vote, ..
        } => {
            let number = Arg::D(i64::from(*number));
            match (detailed, is_self) {
                (true, true) => fill("LOOT_ROLL_YOU_WON", &[Arg::S(link)]),
                (true, false) => fill("LOOT_ROLL_WON", &[Arg::S(name), Arg::S(link)]),
                (false, true) => fill(
                    &format!("LOOT_ROLL_YOU_WON_NO_SPAM_{}", kind(*vote)?.1),
                    &[number, Arg::S(link)],
                ),
                (false, false) => fill(
                    &format!("LOOT_ROLL_WON_NO_SPAM_{}", kind(*vote)?.1),
                    &[Arg::S(name), number, Arg::S(link)],
                ),
            }
        }
        Roll::Vote {
            number,
            kind: vote,
            auto_pass,
            ..
        } if detailed => {
            if (1..=100).contains(number) {
                return fill(
                    &format!("LOOT_ROLL_ROLLED_{}", kind(*vote)?.1),
                    &[Arg::D(i64::from(*number)), Arg::S(link), Arg::S(name)],
                );
            }
            // AC Group.cpp::CountRollVote sends Need selection as (number=0, kind=0).
            let (key, named) = if *number == 0 && *vote == 0 {
                ("LOOT_ROLL_NEED".to_string(), true)
            } else if *vote == 0 {
                match (is_self, *auto_pass) {
                    (true, true) => ("LOOT_ROLL_PASSED_SELF_AUTO".to_string(), false),
                    (true, false) => ("LOOT_ROLL_PASSED_SELF".to_string(), false),
                    (false, true) if female? => ("LOOT_ROLL_PASSED_AUTO_FEMALE".to_string(), true),
                    (false, true) => ("LOOT_ROLL_PASSED_AUTO".to_string(), true),
                    (false, false) => ("LOOT_ROLL_PASSED".to_string(), true),
                }
            } else {
                (format!("LOOT_ROLL_{}", kind(*vote)?.0), true)
            };
            // A selection's own line is its `_SELF` key; the pass keys already carry theirs.
            if named && is_self {
                fill(&format!("{key}_SELF"), &[Arg::S(link)])
            } else if named {
                fill(&key, &[Arg::S(name), Arg::S(link)])
            } else {
                fill(&key, &[Arg::S(link)])
            }
        }
        _ => None,
    }
}
/// Each pending outcome as its chat line once its item and name resolve. `get` is the VM's string
/// lookup; with no VM the lines wait, as the item and name queries do.
pub(super) fn drain(
    pending: &mut Vec<(Roll, u16)>,
    items: &Items,
    names: &NameCache,
    commands: &NetCommands,
    own: Option<u64>,
    chat: &mut ChatLog,
    detailed: bool,
    get: Option<Strings<'_>>,
) {
    let Some(get) = get else {
        return;
    };
    pending.retain_mut(|(roll, tries)| {
        if !detailed && matches!(roll, Roll::Vote { .. }) {
            return false;
        }
        let (item, player) = match &*roll {
            Roll::Vote { item, player, .. } | Roll::Won { item, player, .. } => {
                (item, Some(*player))
            }
            Roll::AllPassed { item } => (item, None),
            Roll::Start { .. } => return false,
        };
        let render = || {
            let t = items.template(item.entry, 0, commands)?;
            let link = format!(
                "|c{}|Hitem:{}:0:0:0:0:{}:{}:0|h[{}]|h|r",
                crate::ui_items::quality_color(t.quality),
                item.entry,
                item.random_property,
                item.suffix_factor,
                t.name
            );
            let is_self = player.is_some() && player == own;
            let needs_name = !is_self
                || matches!(
                    roll,
                    Roll::Vote {
                        number: 1..=100,
                        ..
                    }
                );
            let name = match player {
                Some(g) if needs_name => names.resolve(g, commands)?,
                _ => "",
            };
            line(
                roll,
                name,
                is_self,
                &link,
                detailed,
                player.and_then(|g| names.player_traits(g).map(|(_, _, gender)| gender == 1)),
                get,
            )
        };
        if let Some(text) = render() {
            chat.push_event(ChatEvent::text_only(ChatEventKind::Loot, text));
            false
        } else {
            *tries += 1;
            *tries < 120
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The build-12340 `LOOT_ROLL_*` templates these lines read, as `GlobalStrings.lua` ships them.
    fn strings(key: &str) -> Option<String> {
        Some(
            match key {
                "LOOT_ROLL_ALL_PASSED" => "Everyone passed on: %s",
                "LOOT_ROLL_DISENCHANT" => "%s has selected Disenchant for: %s",
                "LOOT_ROLL_NEED" => "%s has selected Need for: %s",
                "LOOT_ROLL_PASSED_AUTO" => {
                    "%s automatically passed on: %s because he cannot loot that item."
                }
                "LOOT_ROLL_PASSED_AUTO_FEMALE" => {
                    "%s automatically passed on: %s because she cannot loot that item."
                }
                "LOOT_ROLL_PASSED_SELF_AUTO" => {
                    "You automatically passed on: %s because you cannot loot that item."
                }
                "LOOT_ROLL_ROLLED_DE" => "Disenchant Roll - %d for %s by %s",
                "LOOT_ROLL_YOU_WON_NO_SPAM_DE" => "You won: %2$s |cff818181(Disenchant - %1$d)|r",
                _ => return None,
            }
            .to_string(),
        )
    }
    fn item() -> wrathplus_protocol::wrath_roll::Item {
        wrathplus_protocol::wrath_roll::Item {
            guid: u64::MAX,
            slot: u32::MAX,
            entry: 42,
            suffix_factor: u32::MAX,
            random_property: -17,
        }
    }
    #[test]
    fn native_disenchant_dice_vote_win_and_allpassed_templates() {
        let mut roll = Roll::Vote {
            item: item(),
            player: u64::MAX,
            number: 128,
            kind: 3,
            auto_pass: false,
        };
        assert_eq!(
            line(&roll, "Bob", false, "LINK", true, Some(false), &strings).unwrap(),
            "Bob has selected Disenchant for: LINK"
        );
        assert_eq!(
            line(&roll, "Bob", false, "LINK", false, Some(false), &strings),
            None
        );
        if let Roll::Vote { number, .. } = &mut roll {
            *number = 88;
        }
        assert_eq!(
            line(&roll, "Bob", true, "LINK", true, Some(false), &strings).unwrap(),
            "Disenchant Roll - 88 for LINK by Bob"
        );
        let won = Roll::Won {
            item: item(),
            player: u64::MAX,
            number: 88,
            kind: 3,
        };
        assert_eq!(
            line(&won, "Bob", true, "LINK", false, Some(false), &strings).unwrap(),
            "You won: LINK |cff818181(Disenchant - 88)|r"
        );
        assert_eq!(
            line(
                &Roll::AllPassed { item: item() },
                "",
                false,
                "LINK",
                false,
                Some(false),
                &strings
            )
            .unwrap(),
            "Everyone passed on: LINK"
        );
    }
    #[test]
    fn zero_zero_is_need_not_pass_and_automatic_pass_preserves_gender() {
        let mut roll = Roll::Vote {
            item: item(),
            player: 9,
            number: 0,
            kind: 0,
            auto_pass: false,
        };
        assert_eq!(
            line(&roll, "Bob", false, "L", true, None, &strings).unwrap(),
            "Bob has selected Need for: L"
        );
        if let Roll::Vote {
            number, auto_pass, ..
        } = &mut roll
        {
            *number = 128;
            *auto_pass = true;
        }
        assert_eq!(
            line(&roll, "Alice", false, "L", true, Some(true), &strings).unwrap(),
            "Alice automatically passed on: L because she cannot loot that item."
        );
        assert_eq!(
            line(&roll, "Bob", false, "L", true, Some(false), &strings).unwrap(),
            "Bob automatically passed on: L because he cannot loot that item."
        );
        assert!(line(&roll, "Bob", false, "L", true, None, &strings).is_none());
        assert_eq!(
            line(&roll, "", true, "L", true, None, &strings).unwrap(),
            "You automatically passed on: L because you cannot loot that item."
        );
    }
}
