//! The unit a character-sheet verb's token names, through build 12340's resolver (`0x60c1f0`):
//! the active player, our pet, another held unit, or nobody.

use super::Model;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatUnit {
    Player,
    Pet,
    /// A held unit that is neither; `is_player` is the PLAYER typemask some verbs test.
    Other {
        is_player: bool,
    },
    Nobody,
}

impl Model {
    /// The app pushes the player's and the pet's snapshots under those tokens, matched first as
    /// [`Model::unit`] matches a push; any other spelling goes through the resolver's guid.
    pub(crate) fn stat_unit(&self, token: &str) -> StatUnit {
        if token.eq_ignore_ascii_case("player") {
            return StatUnit::Player;
        }
        if token.eq_ignore_ascii_case("pet") {
            return StatUnit::Pet;
        }
        let Some(guid) = self.unit_guid(token) else {
            return StatUnit::Nobody;
        };
        if guid == self.local_guid() {
            StatUnit::Player
        } else if guid == self.unit_guids.pet {
            StatUnit::Pet
        } else {
            match self.unit_by_guid(guid) {
                Some(u) if u.has_object => StatUnit::Other {
                    is_player: u.is_player,
                },
                _ => StatUnit::Nobody,
            }
        }
    }
}
