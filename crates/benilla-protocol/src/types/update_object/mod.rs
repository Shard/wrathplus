//! `SMSG_UPDATE_OBJECT` body decode: the object list, each entry's [`MovementBlock`] and its
//! sparse [`ObjectFields`].



mod fields;
mod movement;

pub use fields::field;
pub use fields::{
    power_display_scale, quest_slot_state, CorpseLook, Field, FieldSpan, NpcFlags,
    NpcService, ObjectFields, OwnerFallback, PlayerSkillSlot, QuestLogSlot, UnitAuraSlot,
    AURA_FLAG_CANCELABLE, AURA_FLAG_EFF_INDEX_MASK, FIELD_PLAYER_SKILL_INFO_1_1,
    PLAYER_EXPLORED_ZONES_SLOTS, PLAYER_QUEST_LOG_SLOTS, PLAYER_SKILL_SLOTS,
    UNIT_AURA_POSITIVE_SLOTS, UNIT_AURA_SLOTS,
};
pub use movement::{CreateSpline, MovementBlock, MoverState, ObjectType};

/// One entry in an `SMSG_UPDATE_OBJECT` object list.
pub enum Object {
    Values {
        guid: u64,
        mask: ObjectFields,
    },
    Movement {
        guid: u64,
        movement: MovementBlock,
    },
    /// `CREATE_OBJECT` or `CREATE_OBJECT2`, which share one wire shape.
    Create {
        guid: u64,
        object_type: ObjectType,
        movement: MovementBlock,
        mask: ObjectFields,
    },
    OutOfRange {
        guids: Vec<u64>,
    },
    Near {
        guids: Vec<u64>,
    },
}

impl Object {
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quest_log_cleared_slot_reads_zero_id() {
        // An abandoned slot: the server zeroes the id and the pair.
        let f = ObjectFields::from_pairs(&[(158, 0), (159, 0), (160, 0)]);
        assert_eq!(f.player_quest_log(0).map(|s| s.quest_id), Some(0));
    }
}
