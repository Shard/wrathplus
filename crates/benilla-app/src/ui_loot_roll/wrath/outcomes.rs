//! Native outcome chat, using build-12340 GlobalStrings.lua LOOT_ROLL_* templates.
use crate::{
    items::Items,
    names::NameCache,
    net::NetCommands,
    ui_chat::{ChatEvent, ChatEventKind, ChatLog},
};
use benilla_protocol::wrath_roll::Roll;

fn kind(vote: u8) -> Option<&'static str> {
    match vote {
        1 => Some("Need"),
        2 => Some("Greed"),
        3 => Some("Disenchant"),
        _ => None,
    }
}
fn line(
    roll: &Roll,
    name: &str,
    is_self: bool,
    link: &str,
    detailed: bool,
    female: Option<bool>,
) -> Option<String> {
    match roll {
        Roll::AllPassed { .. } => Some(format!("Everyone passed on: {link}")),
        Roll::Won {
            number, kind: vote, ..
        } => {
            let who = if is_self { "You" } else { name };
            if detailed {
                Some(format!("{who} won: {link}"))
            } else {
                Some(format!(
                    "{who} won: {link} |cff818181({} - {number})|r",
                    kind(*vote)?
                ))
            }
        }
        Roll::Vote {
            number,
            kind: vote,
            auto_pass,
            ..
        } if detailed => {
            if (1..=100).contains(number) {
                return Some(format!(
                    "{} Roll - {number} for {link} by {name}",
                    kind(*vote)?
                ));
            }
            // AC Group.cpp::CountRollVote sends Need selection as (number=0, kind=0).
            if *number == 0 && *vote == 0 {
                return Some(if is_self {
                    format!("You have selected Need for: {link}")
                } else {
                    format!("{name} has selected Need for: {link}")
                });
            }
            if *vote == 0 {
                return Some(if is_self {
                    if *auto_pass {
                        format!("You automatically passed on: {link} because you cannot loot that item.")
                    } else {
                        format!("You passed on: {link}")
                    }
                } else if *auto_pass {
                    let pronoun = if female? { "she" } else { "he" };
                    format!("{name} automatically passed on: {link} because {pronoun} cannot loot that item.")
                } else {
                    format!("{name} passed on: {link}")
                });
            }
            Some(if is_self {
                format!("You have selected {} for: {link}", kind(*vote)?)
            } else {
                format!("{name} has selected {} for: {link}", kind(*vote)?)
            })
        }
        _ => None,
    }
}
pub(super) fn drain(
    pending: &mut Vec<(Roll, u16)>,
    items: &Items,
    names: &NameCache,
    commands: &NetCommands,
    own: Option<u64>,
    chat: &mut ChatLog,
    detailed: bool,
) {
    pending.retain_mut(|(roll, tries)| {
        if !detailed && matches!(roll, Roll::Vote { .. }) {
            return false;
        }
        let (item, player) = match &*roll {
            Roll::Vote { item, player, .. } | Roll::Won { item, player, .. } => {
                (&*item, Some(*player))
            }
            Roll::AllPassed { item } => (&*item, None),
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
    fn item() -> benilla_protocol::wrath_roll::Item {
        benilla_protocol::wrath_roll::Item {
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
            line(&roll, "Bob", false, "LINK", true, Some(false)).unwrap(),
            "Bob has selected Disenchant for: LINK"
        );
        assert_eq!(line(&roll, "Bob", false, "LINK", false, Some(false)), None);
        if let Roll::Vote { number, .. } = &mut roll {
            *number = 88;
        }
        assert_eq!(
            line(&roll, "Bob", true, "LINK", true, Some(false)).unwrap(),
            "Disenchant Roll - 88 for LINK by Bob"
        );
        let won = Roll::Won {
            item: item(),
            player: u64::MAX,
            number: 88,
            kind: 3,
        };
        assert_eq!(
            line(&won, "Bob", true, "LINK", false, Some(false)).unwrap(),
            "You won: LINK |cff818181(Disenchant - 88)|r"
        );
        assert_eq!(
            line(
                &Roll::AllPassed { item: item() },
                "",
                false,
                "LINK",
                false,
                Some(false)
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
            line(&roll, "Bob", false, "L", true, None).unwrap(),
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
            line(&roll, "Alice", false, "L", true, Some(true)).unwrap(),
            "Alice automatically passed on: L because she cannot loot that item."
        );
        assert_eq!(
            line(&roll, "Bob", false, "L", true, Some(false)).unwrap(),
            "Bob automatically passed on: L because he cannot loot that item."
        );
        assert!(line(&roll, "Bob", false, "L", true, None).is_none());
        assert_eq!(
            line(&roll, "", true, "L", true, None).unwrap(),
            "You automatically passed on: L because you cannot loot that item."
        );
    }
}
