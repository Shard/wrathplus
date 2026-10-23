//! Build 12340's arena-opponent table (`BattlefieldInfo.cpp`): five opponent slots, `arenaN`
//! (`0xbe9f48`), and five pet slots, `arenapetN` (`0xbe9ef8`, a pet and its owner). The world's
//! token resolver reads it; the arena feed (`ui_native::wrath_arena`) fills it.

use bevy::prelude::*;

/// The opponent and pet slots, slot N-1 for token N; 0 is empty.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArenaOpponents {
    pub arena: [u64; 5],
    pub pets: [u64; 5],
    pub pet_owners: [u64; 5],
}

impl ArenaOpponents {
    /// `0x54d100`: the slot an opponent takes, and whether it was another guid's (the info cache
    /// resets, `0x54b840`). A slot whose pet names this owner wins; else the first slot with no
    /// pet whose opponent is this guid or none. `None` when all five are taken.
    pub fn assign(&mut self, guid: u64) -> Option<(usize, bool)> {
        let slot = self
            .pet_owners
            .iter()
            .position(|&o| o == guid)
            .or_else(|| {
                (0..5).find(|&i| self.pets[i] == 0 && (self.arena[i] == guid || self.arena[i] == 0))
            })?;
        let fresh = self.arena[slot] != guid;
        self.arena[slot] = guid;
        Some((slot, fresh))
    }

    /// `0x54d1c0`: the slot a pet of `owner` takes: the first whose opponent is the owner or whose
    /// pet is the owner's; else the first holding neither an opponent nor a pet.
    pub fn assign_pet(&mut self, pet: u64, owner: u64) -> Option<usize> {
        let slot = (0..5)
            .find(|&i| owner != 0 && (self.arena[i] == owner || self.pet_owners[i] == owner))
            .or_else(|| (0..5).find(|&i| self.arena[i] == 0 && self.pets[i] == 0))?;
        self.pets[slot] = pet;
        self.pet_owners[slot] = owner;
        Some(slot)
    }

    /// The pet slot holding `guid`, if any.
    pub fn pet_slot_of(&self, guid: u64) -> Option<usize> {
        (guid != 0).then(|| self.pets.iter().position(|&g| g == guid))?
    }

    /// The opponent slot holding `guid`, if any.
    pub fn slot_of(&self, guid: u64) -> Option<usize> {
        (guid != 0).then(|| self.arena.iter().position(|&g| g == guid))?
    }

    /// `0x549630`, `GetNumArenaOpponents`: the slots before the first holding neither.
    pub fn count(&self) -> usize {
        (0..5)
            .take_while(|&i| self.arena[i] != 0 || self.pets[i] != 0)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sighting order fills slots 1, 2, 3; a returning opponent keeps its slot, unchanged.
    #[test]
    fn opponents_take_the_first_free_slot_and_keep_it() {
        let mut t = ArenaOpponents::default();
        assert_eq!(t.assign(0xA), Some((0, true)));
        assert_eq!(t.assign(0xB), Some((1, true)));
        assert_eq!(
            t.assign(0xA),
            Some((0, false)),
            "seen again: same slot, no reset"
        );
        assert_eq!(t.assign(0xC), Some((2, true)));
        assert_eq!(t.count(), 3);
        assert_eq!(t.slot_of(0xB), Some(1));
        assert_eq!(t.slot_of(0), None);
    }

    /// A slot holding a pet is skipped by a stranger, but its owner lands on it; five is the cap.
    #[test]
    fn a_pets_slot_belongs_to_its_owner() {
        let mut t = ArenaOpponents {
            pets: [0x70, 0, 0, 0, 0],
            pet_owners: [0xD, 0, 0, 0, 0],
            ..Default::default()
        };
        assert_eq!(
            t.assign(0xE),
            Some((1, true)),
            "slot 1 has a pet that is not E's"
        );
        assert_eq!(t.assign(0xD), Some((0, true)), "D owns slot 1's pet");
        for g in [0x1, 0x2, 0x3] {
            assert!(t.assign(g).is_some());
        }
        assert_eq!(t.assign(0x4), None, "all five taken");
    }

    /// A pet lands on its owner's slot, else its owner's earlier pet's, else the first empty one.
    #[test]
    fn a_pet_follows_its_owner() {
        let mut t = ArenaOpponents::default();
        t.assign(0xA);
        t.assign(0xB);
        assert_eq!(t.assign_pet(0x7B, 0xB), Some(1), "B's slot");
        assert_eq!(
            t.assign_pet(0x7C, 0xC),
            Some(2),
            "C is unseen: the first empty slot"
        );
        assert_eq!(
            t.assign(0xC),
            Some((2, true)),
            "and C then joins its pet's slot"
        );
        assert_eq!(
            t.assign_pet(0x7D, 0xC),
            Some(2),
            "C's next pet replaces the first"
        );
        assert_eq!(t.pet_slot_of(0x7D), Some(2));
        assert_eq!(t.pet_slot_of(0x7C), None);
    }
}
