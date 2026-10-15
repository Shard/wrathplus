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
    /// Native precomputed reasons; unknown ownership metadata must not be guessed.
    pub reasons: [Option<u8>; 3],
    pub disenchant_skill_required: Option<i32>,
}

impl Entry {
    /// AzerothCore LootMgr.h::RollMask uses bit N for RollVote N (0 pass through 3 disenchant).
    pub fn permits(&self, vote: u8) -> bool {
        vote <= 3 && self.vote_mask & (1u8 << vote) != 0
    }
}

/// Build-12340 `0x6fa7a0` reason writer. `can_receive` is the result of the ownership-limit
/// helper `0x754dd0` (entry max count and ItemLimitCategory), NOT a usability/class check.
/// A missing ownership snapshot stays unknown when Need is disabled. Neither these reasons
/// nor template flags confer vote authority; that remains the server's mask.
/// `0x5889d0` returns the required disenchant skill as a signed template value (+0x1e0).
pub fn native_reasons(
    vote_mask: u8,
    flags2: u32,
    required_disenchant_skill: i32,
    can_receive: Option<bool>,
) -> [Option<u8>; 3] {
    let need = if vote_mask & 2 != 0 {
        Some(0)
    } else {
        can_receive.map(|allowed| {
            if !allowed {
                2
            } else if flags2 & 0x100 != 0 {
                5
            } else {
                1
            }
        })
    };
    let greed = if vote_mask & 4 != 0 { 0 } else { 2 };
    let disenchant = if vote_mask & 8 != 0 {
        0
    } else if required_disenchant_skill < 0 {
        3
    } else {
        4
    };
    [need, Some(greed), Some(disenchant)]
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
    fn native_reason_branches_preserve_signed_skill_and_unknown_ownership() {
        for mask in 0..=u8::MAX {
            for flags2 in [0, 0x100, 0xffff_feff, u32::MAX] {
                for skill in [i32::MIN, -1, 0, 1, i32::MAX] {
                    for capacity in [None, Some(false), Some(true)] {
                        let reasons = native_reasons(mask, flags2, skill, capacity);
                        assert_eq!(
                            reasons[0],
                            if mask & 2 != 0 {
                                Some(0)
                            } else {
                                capacity.map(|ok| {
                                    if !ok {
                                        2
                                    } else if flags2 & 0x100 != 0 {
                                        5
                                    } else {
                                        1
                                    }
                                })
                            }
                        );
                        assert_eq!(reasons[1], Some(if mask & 4 != 0 { 0 } else { 2 }));
                        assert_eq!(
                            reasons[2],
                            Some(if mask & 8 != 0 {
                                0
                            } else if skill < 0 {
                                3
                            } else {
                                4
                            })
                        );
                    }
                }
            }
        }
        assert_eq!(
            native_reasons(0, 0, -1, Some(false)),
            [Some(2), Some(2), Some(3)]
        );
        assert_eq!(
            native_reasons(0, 0x100, 0, Some(true)),
            [Some(5), Some(2), Some(4)]
        );
        assert_eq!(native_reasons(0, 0, 0, Some(true))[0], Some(1));
        assert_eq!(native_reasons(0, 0x100, 0, None)[0], None);
    }

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
