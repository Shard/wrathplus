//! Native build-12340 roll snapshot. Separate from the 1.12 six-value roll entry: stock
//! LootFrame.lua:341 reads twelve values, including server-authorized vote eligibility.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    pub roll_id: u32,
    pub guid: u64,
    pub slot: u32,
    pub map: u32,
    pub entry: u32,
    pub count: u32,
    pub suffix_factor: u32,
    pub random_property: i32,
    pub vote_mask: u8,
    pub texture: Option<String>,
    pub name: Option<String>,
    pub quality: Option<u32>,
    pub bind_on_pickup: bool,
    pub time_left_ms: u32,
    pub link: Option<String>,
    /// Ineligibility reasons require item/class/skill metadata; absence is not invented authority.
    pub reasons: [Option<u8>; 3],
    pub disenchant_skill_required: Option<u32>,
}

impl Entry {
    /// AzerothCore LootMgr.h::RollMask uses bit N for RollVote N (0 pass through 3 disenchant).
    pub fn permits(&self, vote: u8) -> bool {
        vote <= 3 && self.vote_mask & (1u8 << vote) != 0
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub rolls: Vec<Entry>,
}

impl super::super::UiScript {
    pub fn set_wrath_loot_rolls(&mut self, state: State) {
        self.model_mut().wrath_loot_rolls = state;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn server_mask_is_the_only_vote_authority_and_unknown_bits_grant_nothing() {
        for mask in 0..=u8::MAX {
            let entry = Entry {
                vote_mask: mask,
                ..Default::default()
            };
            for vote in 0..=u8::MAX {
                assert_eq!(entry.permits(vote), vote < 4 && mask & (1 << vote) != 0);
            }
        }
    }
}
