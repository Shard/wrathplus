//! Stateful semantic adapter from Wrath build-12340 packets to expansion-neutral session events.
//!
//! The wire codecs stay in profile-native modules. This module owns the state required by sparse
//! values updates. Descriptors retain their native Wrath12340 layout without projection.

use std::collections::HashMap;
use std::fmt;

use crate::events::{EntityKind, MoveSpeeds, SessionEvent};
use crate::messages::FieldLayout;
use crate::messages::{
    AttackSwingError, CreateSpline, MoverState, ObjectFields, ObjectType as VanillaObjectType,
    TransportPose,
};
use crate::wire::Vector3d;
use crate::wrath_combat::{
    decode_ai_reaction, decode_attack_start, decode_attack_stop, decode_attack_swing_refusal,
    decode_attacker_state, AiReaction, AttackStop, AttackSwingRefusal, AttackerState,
    CombatCodecError, SMSG_AI_REACTION, SMSG_ATTACKER_STATE_UPDATE, SMSG_ATTACK_START,
    SMSG_ATTACK_STOP, SMSG_ATTACK_SWING_BAD_FACING, SMSG_ATTACK_SWING_CANT_ATTACK,
    SMSG_ATTACK_SWING_DEAD_TARGET, SMSG_ATTACK_SWING_NOT_IN_RANGE,
};
use crate::wrath_gameplay::{
    CodecError, DestroyObject, MovementBlock, MovementPosition, ObjectType, ObjectUpdate,
    UpdateFields, UpdateObject, MOVEMENT_FLAG_ON_TRANSPORT, MOVEMENT_FLAG_SPLINE_ENABLED,
    UPDATE_FLAG_SELF,
};
use crate::wrath_items::{decode_native_item_query_response, ItemCodecError};
use crate::wrath_loot::{
    self, InventoryChangeFailure, LootCodecError, LootMoneyNotify, LootResponse, ServerLootPacket,
    WrathItemPushResult, WrathLootItem,
};
use crate::wrath_movement::decode_monster_move;
use crate::wrath_world::WrathServerPacket;

pub const SMSG_NEW_WORLD: u16 = 0x003e;
pub const SMSG_LOGIN_SETTIMESPEED: u16 = 0x0042;
pub const SMSG_ITEM_QUERY_SINGLE_RESPONSE: u16 = 0x0058;
pub const SMSG_UPDATE_OBJECT: u16 = 0x00a9;
pub const SMSG_DESTROY_OBJECT: u16 = 0x00aa;
pub const SMSG_MONSTER_MOVE: u16 = 0x00dd;
pub const SMSG_COMPRESSED_UPDATE_OBJECT: u16 = 0x01f6;
pub const SMSG_LOGIN_VERIFY_WORLD: u16 = 0x0236;
pub const SMSG_MONSTER_MOVE_TRANSPORT: u16 = 0x02ae;
pub const SMSG_WARDEN_DATA: u16 = 0x02e6;
pub const SMSG_TIME_SYNC_REQ: u16 = 0x0390;
pub const CMSG_TIME_SYNC_RESP: u32 = 0x0391;

/// A client packet the caller must send through `WrathWorldWriter`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrathReply {
    pub opcode: u32,
    pub body: Vec<u8>,
}

/// Wrath information that has no lossless [`SessionEvent`] representation.
#[derive(Debug, Clone, PartialEq)]
pub enum WrathEvent {
    /// Channel roster retains the leading native type absent from the shared roster event.
    ChannelList(crate::wrath_channel::ChannelList),
    /// The original ordered object block. This retains flags2, nine speeds, seats, vehicles,
    /// packed rotations, Wrath spline details, create-vs-create2, Near, and destroy metadata.
    Object(Box<ObjectUpdate>),
    Destroy {
        guid: u64,
        on_death: bool,
    },
    TimeSyncRequest {
        counter: u32,
    },
    /// The native stop retains `NowDead`, which the expansion-neutral stop edge cannot carry.
    AttackStop(AttackStop),
    /// The native refusal distinguishes dead targets from otherwise unattackable targets.
    AttackSwingRefusal(AttackSwingRefusal),
    /// Wrath's overkill, school-mask blocks, conditional arrays, `u8` target state, and hit-bit
    /// meanings are incompatible with the Vanilla [`crate::messages::AttackerState`].
    AttackerState(Box<AttackerState>),
    /// Retained alongside the equivalent neutral event for native consumers.
    AiReaction(AiReaction),
    /// Native loot rows retain Wrath's random suffix word, absent from the neutral row.
    LootResponse {
        guid: u64,
        gold: u32,
        items: Vec<WrathLootItem>,
    },
    /// Native money notification retains the server's solo/share presentation selector.
    LootMoneyNotify(LootMoneyNotify),
    /// Native item receipt retains signed random-property and authoritative inventory total.
    ItemPushResult(WrathItemPushResult),
    /// Native refusal retains the second item, bag subclass, and reason-specific tail.
    InventoryFailure(InventoryChangeFailure),
}

/// Result of dispatching one already-framed world packet.
#[derive(Debug, Clone)]
pub enum WrathDispatch {
    Handled {
        session_events: Vec<SessionEvent>,
        wrath_events: Vec<WrathEvent>,
        replies: Vec<WrathReply>,
    },
    Unhandled {
        opcode: u16,
    },
}

#[derive(Debug)]
pub enum WrathEventError {
    Codec {
        opcode: u16,
        source: CodecError,
    },
    CombatCodec {
        opcode: u16,
        source: CombatCodecError,
    },
    ItemCodec {
        opcode: u16,
        source: ItemCodecError,
    },
    LootCodec {
        opcode: u16,
        source: LootCodecError,
    },
    Malformed {
        opcode: u16,
        reason: &'static str,
    },
    WardenRequired,
}

impl fmt::Display for WrathEventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Codec { opcode, source } => write!(f, "Wrath opcode {opcode:#05x}: {source}"),
            Self::CombatCodec { opcode, source } => {
                write!(f, "Wrath opcode {opcode:#05x}: {source}")
            }
            Self::ItemCodec { opcode, source } => {
                write!(f, "Wrath opcode {opcode:#05x}: {source}")
            }
            Self::LootCodec { opcode, source } => {
                write!(f, "Wrath opcode {opcode:#05x}: {source}")
            }
            Self::Malformed { opcode, reason } => {
                write!(f, "malformed Wrath opcode {opcode:#05x}: {reason}")
            }
            Self::WardenRequired => write!(f, "the server requires unsupported Warden"),
        }
    }
}

impl std::error::Error for WrathEventError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Codec { source, .. } => Some(source),
            Self::CombatCodec { source, .. } => Some(source),
            Self::ItemCodec { source, .. } => Some(source),
            Self::LootCodec { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct TrackedObject {
    object_type: ObjectType,
    values: Vec<u32>,
}

/// Stateful packet dispatcher. One instance belongs to one authenticated world session.
#[derive(Debug, Default)]
pub struct WrathEventAdapter {
    objects: HashMap<u64, TrackedObject>,
    /// Our player, from its self create; spell-go confirms our casts against it.
    self_guid: Option<u64>,
    /// Visible auras, which 3.3.5 sends as packets, as the 1.12 descriptor block.
    auras: crate::wrath_auras::AuraTracker,
}

impl WrathEventAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.objects.clear();
        self.self_guid = None;
        self.auras.reset();
    }

    pub fn object_type(&self, guid: u64) -> Option<ObjectType> {
        self.objects.get(&guid).map(|object| object.object_type)
    }

    /// Dispatch a framed packet. `client_time_ms` must use the same wrapping monotonic millisecond
    /// clock as outbound movement timestamps and is used only for `CMSG_TIME_SYNC_RESP`.
    pub fn dispatch_packet(
        &mut self,
        packet: &WrathServerPacket,
        client_time_ms: u32,
    ) -> Result<WrathDispatch, WrathEventError> {
        self.dispatch(packet.opcode, &packet.body, client_time_ms)
    }

    pub fn dispatch(
        &mut self,
        opcode: u16,
        body: &[u8],
        client_time_ms: u32,
    ) -> Result<WrathDispatch, WrathEventError> {
        match opcode {
            opcode if crate::wrath_inventory::OPCODES.contains(&opcode) => Ok(match crate::wrath_inventory::decode(opcode, body) {
                Ok(event) => handled(vec![event], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            opcode if crate::wrath_faction::OPCODES.contains(&opcode) => Ok(match crate::wrath_faction::decode(opcode, body) {
                Ok(event) => handled(vec![event], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            opcode if crate::wrath_inspect::OPCODES.contains(&opcode) => Ok(match crate::wrath_inspect::decode(opcode, body) {
                Ok(event) => handled(vec![event], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            opcode if crate::wrath_session_meta::OPCODES.contains(&opcode) => Ok(match crate::wrath_session_meta::decode(opcode, body) {
                Ok(event) => handled(vec![event], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            opcode if crate::wrath_death::OPCODES.contains(&opcode) => Ok(match crate::wrath_death::decode(opcode, body) {
                Ok(event) => handled(vec![event], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            opcode if crate::wrath_contacts::OPCODES.contains(&opcode) => Ok(match crate::wrath_contacts::decode(opcode, body) {
                Ok(events) => handled(events, Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            opcode if crate::wrath_battleground::OPCODES.contains(&opcode) => Ok(match crate::wrath_battleground::decode(opcode, body) {
                Ok(event) => handled(vec![event], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            opcode if crate::wrath_chat_notices::OPCODES.contains(&opcode) => Ok(match crate::wrath_chat_notices::decode(opcode, body) {
                Ok(events) => handled(events, Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            crate::wrath_channel::SMSG_CHANNEL_NOTIFY if !matches!(body.first(), Some(2 | 3)) => {
                Ok(match crate::wrath_chat_notices::decode_channel_notify(body) {
                    Ok(event) => handled(vec![event], Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            opcode if crate::wrath_party::OPCODES.contains(&opcode) => Ok(match crate::wrath_party::decode(opcode, body) {
                Ok(event) => handled(vec![event], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            opcode if crate::wrath_quest::OPCODES.contains(&opcode) => Ok(match crate::wrath_quest::decode(opcode, body) {
                Ok(packet) => handled(vec![SessionEvent::WrathQuest { packet }], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            opcode if crate::wrath_calendar::OPCODES.contains(&opcode) => Ok(match crate::wrath_calendar::decode(opcode, body) {
                Ok(packet) => handled(vec![SessionEvent::WrathCalendar { packet }], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            opcode if crate::wrath_achievement::OPCODES.contains(&opcode) => Ok(match crate::wrath_achievement::decode(opcode, body) {
                Ok(packet) => handled(vec![SessionEvent::WrathAchievement { packet }], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            opcode if crate::wrath_exchange::OPCODES.contains(&opcode) => Ok(match crate::wrath_exchange::decode(opcode, body) {
                Ok(event) => handled(vec![event], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            0x0214 | 0x0147| 0x02cb | 0x02fa | 0x031e | 0x031f | 0x0320 | 0x0329 | 0x032b | 0x033b | 0x0396 | 0x04eb | 0x0506 => Ok(match crate::wrath_instance_control::decode(opcode, body) {
                Ok(packet) => handled(vec![SessionEvent::WrathInstanceControl { packet }], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            0x0402 => Ok(match crate::wrath_mirror_image::decode(body) {
                Ok(packet) => handled(vec![SessionEvent::WrathMirrorImage { packet }], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            0x04f7 => Ok(match crate::wrath_ui_timer::decode(body) {
                Ok(packet) => handled(vec![SessionEvent::WrathUiTimer { packet }], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            0x0133 => Ok(match crate::wrath_spell_failure::decode_spell(body) {
                Ok(packet) => handled(vec![SessionEvent::WrathSpellFailure(packet)], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            0x0138 => Ok(match crate::wrath_spell_failure::decode_pet(body) {
                Ok(packet) => handled(vec![SessionEvent::WrathPetCastFailed(packet)], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            0x01fc => Ok(match crate::wrath_environment::decode_damage(body) {
                Ok(packet) => handled(vec![SessionEvent::WrathEnvironmentalDamage(packet)], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            0x02bd => Ok(match crate::wrath_environment::decode_durability(body) {
                Ok(()) => handled(vec![SessionEvent::DurabilityDamageDeath], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            0x0063 | 0x0065 => Ok(match crate::wrath_who::decode(opcode, body) {
                Ok(packet) => handled(vec![SessionEvent::WrathWho { packet }], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            0x0501 => Ok(match crate::wrath_quests_completed::decode(body) {
                Ok(packet) => handled(vec![SessionEvent::WrathQuestsCompleted { packet }], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            0x0297 => {
                Ok(match crate::wrath_mail_mutations::decode(opcode, body) {
                    Ok(packet) => handled(vec![SessionEvent::WrathMailMutation { packet }], Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            0x0053 => Ok(match crate::wrath_pet_name::decode(body) {
                Ok(response) => handled(vec![SessionEvent::WrathPetName { response }], Vec::new(), Vec::new()),
                Err(_) => WrathDispatch::Unhandled { opcode },
            }),
            0x0096 | 0x03b3 => {
                Ok(match crate::wrath_chat::decode(opcode, body) {
                    // Raw capture only: App's native feed (`ui_chat::native_message`) delivers it, and a
                    // shared `Chat` would print every line twice.
                    Ok(message) => handled(vec![SessionEvent::WrathChat { message }], Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            0x034c | 0x034e | 0x0350 | 0x035b => {
                Ok(match crate::wrath_arena_team::decode(opcode, body) {
                    Ok(packet) => handled(vec![SessionEvent::WrathArenaTeam { packet }], Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            0x0055 | 0x0083 | 0x0088 | 0x008a | 0x0092 | 0x0093 => {
                Ok(match crate::wrath_guild::decode(opcode, body) {
                    Ok(packet) => handled(vec![SessionEvent::WrathGuild { packet }], Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            crate::wrath_stable::MSG_LIST_STABLED_PETS | crate::wrath_stable::SMSG_STABLE_RESULT => {
                Ok(match crate::wrath_stable::decode(opcode, body) {
                    Ok(packet) => handled(vec![SessionEvent::WrathStable { packet }], Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            crate::wrath_guild_bank::MSG_GUILD_PERMISSIONS
            | crate::wrath_guild_bank::MSG_GUILD_BANK_MONEY_WITHDRAWN => {
                Ok(match crate::wrath_guild_bank::decode(opcode, body) {
                    Ok(packet) => handled(vec![SessionEvent::WrathGuildBank { packet }], Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            crate::wrath_instance::SMSG_RAID_INSTANCE_INFO => {
                Ok(match crate::wrath_instance::decode(body) {
                    Ok(info) => handled(vec![SessionEvent::WrathRaidInstanceInfo { info }], Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            opcode if crate::wrath_lfg::handles(opcode) => {
                Ok(match crate::wrath_lfg::decode(opcode, body) {
                    Ok(packet) => handled(vec![SessionEvent::WrathLfg { packet }], Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            crate::wrath_pet::SMSG_PET_SPELLS => {
                Ok(match crate::wrath_pet::decode(body) {
                    Ok(spells) => handled(vec![SessionEvent::WrathPetSpells { spells: Box::new(spells) }], Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            crate::wrath_channel::SMSG_CHANNEL_NOTIFY if matches!(body.first(), Some(2 | 3)) => {
                Ok(match crate::wrath_channel::decode_notify(body) {
                    Ok(notify) => handled(vec![notify.session_event(), SessionEvent::WrathChannelNotify { notify }], Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            crate::wrath_channel::SMSG_CHANNEL_LIST => {
                Ok(match crate::wrath_channel::decode(body) {
                    Ok(list) => handled(vec![list.session_event(), SessionEvent::WrathChannelList { roster: list.clone() }], vec![WrathEvent::ChannelList(list)], Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            opcode if crate::wrath_roll::handles(opcode) => {
                Ok(match crate::wrath_roll::decode(opcode, body) {
                    Ok(roll) => handled(vec![SessionEvent::WrathLootRoll(roll)], Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            opcode if crate::wrath_group::handles(opcode) => {
                Ok(match crate::wrath_group::decode(opcode, body) {
                    Ok(packet) => handled(
                        vec![SessionEvent::WrathGroup { packet: Box::new(packet) }],
                        Vec::new(),
                        Vec::new(),
                    ),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            opcode if crate::wrath_mail::handles(opcode) => {
                Ok(match crate::wrath_mail::decode(opcode, body) {
                    Ok(events) => handled(events, Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            opcode if crate::wrath_taxi::handles(opcode) => {
                Ok(match crate::wrath_taxi::decode(opcode, body) {
                    Ok(events) => handled(events, Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            opcode if crate::wrath_bind::handles(opcode) => {
                Ok(match crate::wrath_bind::decode(opcode, body) {
                    Ok(events) => handled(events, Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            opcode if crate::wrath_misc::handles(opcode) => {
                Ok(match crate::wrath_misc::decode(opcode, body) {
                    Ok(events) => handled(events, Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            opcode if crate::wrath_progress::handles(opcode) => {
                Ok(match crate::wrath_progress::decode(opcode, body) {
                    Ok(events) => handled(events, Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            opcode if crate::wrath_logs::handles(opcode) => {
                Ok(match crate::wrath_logs::decode(opcode, body) {
                    Ok(events) => handled(events, Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            opcode if crate::wrath_spell_control::handles(opcode) => {
                Ok(match crate::wrath_spell_control::decode(opcode, body) {
                    Ok(events) => handled(events, Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            opcode if crate::wrath_spline::handles(opcode) => {
                Ok(match crate::wrath_spline::decode(opcode, body) {
                    Ok(events) => handled(events, Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            SMSG_WARDEN_DATA => Err(WrathEventError::WardenRequired),
            wrath_loot::SMSG_LOOT_RESPONSE
            | wrath_loot::SMSG_LOOT_RELEASE_RESPONSE
            | wrath_loot::SMSG_LOOT_REMOVED
            | wrath_loot::SMSG_LOOT_MONEY_NOTIFY
            | wrath_loot::SMSG_LOOT_CLEAR_MONEY
            | wrath_loot::SMSG_ITEM_PUSH_RESULT
            | wrath_loot::SMSG_INVENTORY_CHANGE_FAILURE => {
                let packet = wrath_loot::decode(opcode, body)
                    .map_err(|source| WrathEventError::LootCodec { opcode, source })?;
                Ok(adapt_loot(packet))
            }
            crate::wrath_combat::SMSG_CANCEL_COMBAT => {
                crate::wrath_combat::decode_cancel_combat(body)
                    .map_err(|source| WrathEventError::CombatCodec { opcode, source })?;
                Ok(handled(
                    vec![SessionEvent::CancelCombat],
                    Vec::new(),
                    Vec::new(),
                ))
            }
            crate::wrath_combat::SMSG_STAND_STATE_UPDATE => {
                let state = crate::wrath_combat::decode_stand_state_update(body)
                    .map_err(|source| WrathEventError::CombatCodec { opcode, source })?;
                Ok(handled(
                    vec![SessionEvent::StandStateUpdate { state }],
                    Vec::new(),
                    Vec::new(),
                ))
            }
            SMSG_ATTACK_START => {
                let start = decode_attack_start(body)
                    .map_err(|source| WrathEventError::CombatCodec { opcode, source })?;
                Ok(handled(
                    vec![SessionEvent::AttackStart {
                        attacker: start.attacker,
                        victim: start.victim,
                    }],
                    Vec::new(),
                    Vec::new(),
                ))
            }
            SMSG_ATTACK_STOP => {
                let stop = decode_attack_stop(body)
                    .map_err(|source| WrathEventError::CombatCodec { opcode, source })?;
                Ok(handled(
                    vec![SessionEvent::AttackStop {
                        attacker: stop.attacker,
                        victim: stop.victim,
                    }],
                    vec![WrathEvent::AttackStop(stop)],
                    Vec::new(),
                ))
            }
            SMSG_ATTACK_SWING_NOT_IN_RANGE
            | SMSG_ATTACK_SWING_BAD_FACING
            | SMSG_ATTACK_SWING_DEAD_TARGET
            | SMSG_ATTACK_SWING_CANT_ATTACK => {
                let refusal = decode_attack_swing_refusal(opcode, body)
                    .map_err(|source| WrathEventError::CombatCodec { opcode, source })?;
                let neutral = match refusal {
                    AttackSwingRefusal::NotInRange => AttackSwingError::NotInRange,
                    AttackSwingRefusal::BadFacing => AttackSwingError::BadFacing,
                    AttackSwingRefusal::DeadTarget | AttackSwingRefusal::CantAttack => {
                        AttackSwingError::DeadOrUnattackable
                    }
                };
                Ok(handled(
                    vec![SessionEvent::AttackSwingError(neutral)],
                    vec![WrathEvent::AttackSwingRefusal(refusal)],
                    Vec::new(),
                ))
            }
            SMSG_ATTACKER_STATE_UPDATE => {
                let state = decode_attacker_state(body)
                    .map_err(|source| WrathEventError::CombatCodec { opcode, source })?;
                Ok(handled(
                    vec![SessionEvent::WrathAttackerState(Box::new(state.clone()))],
                    vec![WrathEvent::AttackerState(Box::new(state))],
                    Vec::new(),
                ))
            }
            SMSG_AI_REACTION => {
                let reaction = decode_ai_reaction(body)
                    .map_err(|source| WrathEventError::CombatCodec { opcode, source })?;
                Ok(handled(
                    vec![SessionEvent::AiReaction {
                        unit: reaction.unit,
                        reaction: reaction.reaction,
                    }],
                    vec![WrathEvent::AiReaction(reaction)],
                    Vec::new(),
                ))
            }
            SMSG_ITEM_QUERY_SINGLE_RESPONSE => {
                let (entry, template) = decode_native_item_query_response(body)
                    .map_err(|source| WrathEventError::ItemCodec { opcode, source })?;
                let (info, metadata) = match template {
                    Some(template) => (Some(Box::new(template.info)), Some(template.roll)),
                    None => (None, None),
                };
                Ok(handled(
                    vec![
                        SessionEvent::ItemTemplate { entry, info },
                        SessionEvent::WrathItemRollMetadata { entry, metadata },
                    ],
                    Vec::new(),
                    Vec::new(),
                ))
            }
            SMSG_UPDATE_OBJECT | SMSG_COMPRESSED_UPDATE_OBJECT => {
                let parsed = if opcode == SMSG_UPDATE_OBJECT {
                    UpdateObject::from_bytes_with_object_types(body, |guid| self.object_type(guid))
                } else {
                    UpdateObject::from_compressed_bytes_with_object_types(body, |guid| {
                        self.object_type(guid)
                    })
                }
                .map_err(|source| WrathEventError::Codec { opcode, source })?;
                if parsed.objects.iter().any(|object| {
                    matches!(
                        object,
                        ObjectUpdate::Movement { movement, .. }
                            if movement.update_flags & UPDATE_FLAG_SELF != 0
                    )
                }) {
                    return Err(malformed(
                        opcode,
                        "self movement update has no lossless session-event representation",
                    ));
                }
                Ok(self.apply_update(parsed))
            }
            SMSG_DESTROY_OBJECT => {
                let destroy = DestroyObject::from_bytes(body)
                    .map_err(|source| WrathEventError::Codec { opcode, source })?;
                self.objects.remove(&destroy.guid);
                Ok(handled(
                    vec![SessionEvent::ObjectDestroyed(destroy.guid)],
                    vec![WrathEvent::Destroy {
                        guid: destroy.guid,
                        on_death: destroy.on_death,
                    }],
                    Vec::new(),
                ))
            }
            SMSG_MONSTER_MOVE | SMSG_MONSTER_MOVE_TRANSPORT => {
                let event = decode_monster_move(body, opcode == SMSG_MONSTER_MOVE_TRANSPORT)
                    .map_err(|source| WrathEventError::Codec { opcode, source })?;
                Ok(handled(vec![event], Vec::new(), Vec::new()))
            }
            SMSG_LOGIN_VERIFY_WORLD | SMSG_NEW_WORLD => {
                let (map_id, position, orientation) = world_pose(opcode, body)?;
                Ok(handled(
                    vec![SessionEvent::Worldport {
                        map_id,
                        position,
                        orientation,
                        needs_ack: opcode == SMSG_NEW_WORLD,
                    }],
                    Vec::new(),
                    Vec::new(),
                ))
            }
            SMSG_LOGIN_SETTIMESPEED => {
                if body.len() != 12 {
                    return Err(malformed(opcode, "expected 12-byte packed time/speed body"));
                }
                let datetime = le_u32(&body[0..4]);
                let timescale = f32::from_bits(le_u32(&body[4..8]));
                let day = ((datetime >> 14) & 0x3f) + 1;
                let month = ((datetime >> 20) & 0x0f) + 1;
                let year = (datetime >> 24) & 0x1f;
                Ok(handled(
                    vec![SessionEvent::TimeSpeed {
                        hours: ((datetime >> 6) & 0x1f) as u8,
                        minutes: (datetime & 0x3f) as u8,
                        day_serial: year * 372 + month * 31 + day,
                        timescale,
                    }],
                    Vec::new(),
                    Vec::new(),
                ))
            }
            SMSG_TIME_SYNC_REQ => {
                if body.len() != 4 {
                    return Err(malformed(opcode, "expected one u32 counter"));
                }
                let counter = le_u32(body);
                let mut response = Vec::with_capacity(8);
                response.extend(counter.to_le_bytes());
                response.extend(client_time_ms.to_le_bytes());
                Ok(handled(
                    Vec::new(),
                    vec![WrathEvent::TimeSyncRequest { counter }],
                    vec![WrathReply {
                        opcode: CMSG_TIME_SYNC_RESP,
                        body: response,
                    }],
                ))
            }
            crate::wrath_auras::SMSG_AURA_UPDATE_ALL | crate::wrath_auras::SMSG_AURA_UPDATE => {
                Ok(match self.auras.decode(opcode, body, self.self_guid) {
                    Ok(events) => handled(events, Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            opcode if crate::wrath_trainer::handles(opcode) => {
                Ok(match crate::wrath_trainer::decode(opcode, body) {
                    Ok(events) => handled(events, Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            opcode if crate::wrath_vendor::handles(opcode) => {
                Ok(match crate::wrath_vendor::decode(opcode, body) {
                    Ok(events) => handled(events, Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            opcode if crate::wrath_npc::handles(opcode) => {
                Ok(match crate::wrath_npc::decode(opcode, body) {
                    Ok(events) => handled(events, Vec::new(), Vec::new()),
                    Err(_) => WrathDispatch::Unhandled { opcode },
                })
            }
            opcode if crate::wrath_session::handles(opcode) => {
                // A message that does not decode is dropped, never fatal to the session.
                Ok(
                    match crate::wrath_session::decode(opcode, body, self.self_guid) {
                        Ok(events) => handled(events, Vec::new(), Vec::new()),
                        Err(_) => WrathDispatch::Unhandled { opcode },
                    },
                )
            }
            _ => Ok(WrathDispatch::Unhandled { opcode }),
        }
    }

    fn apply_update(&mut self, update: UpdateObject) -> WrathDispatch {
        let mut session_events = Vec::new();
        let mut wrath_events = Vec::with_capacity(update.objects.len());
        for block in update.objects {
            wrath_events.push(WrathEvent::Object(Box::new(block.clone())));
            match block {
                ObjectUpdate::Create {
                    guid,
                    object_type,
                    movement,
                    fields,
                    ..
                } => {
                    if movement.update_flags & UPDATE_FLAG_SELF != 0 {
                        self.self_guid = Some(guid);
                    }
                    self.auras.forget(guid);
                    let state = TrackedObject::from_create(object_type, &fields);
                    let native = native_fields(&state, true);
                    self.objects.insert(guid, state);
                    if matches!(object_type, ObjectType::Item | ObjectType::Container) {
                        session_events.push(SessionEvent::ItemCreate {
                            guid,
                            container: object_type == ObjectType::Container,
                            fields: native,
                        });
                    } else if let Some((position, orientation)) = placement(&movement) {
                        let display_id = native_display(object_type, &native);
                        let scale = native
                            .object_scale_x()
                            .filter(|value| *value > 0.0)
                            .unwrap_or(1.0);
                        session_events.push(SessionEvent::ObjectCreate {
                            guid,
                            kind: entity_kind(object_type),
                            display_id,
                            position,
                            orientation,
                            scale,
                            speeds: project_speeds(movement.speeds),
                            mover: project_mover(&movement),
                            transport_progress: movement.transport_progress,
                            transport: project_transport(&movement),
                            spline: project_spline(&movement),
                            fields: native,
                        });
                    }
                }
                ObjectUpdate::Values { guid, fields } => {
                    if let Some(state) = self.objects.get_mut(&guid) {
                        let delta = native_delta(state, &fields);
                        state.apply(&fields);
                        if !delta.is_empty() {
                            session_events.push(SessionEvent::ObjectValues {
                                guid,
                                fields: delta,
                            });
                        }
                    }
                }
                ObjectUpdate::Movement { guid, movement } => {
                    if let Some((position, orientation)) = placement(&movement) {
                        session_events.push(SessionEvent::ObjectMove {
                            guid,
                            position,
                            orientation,
                        });
                    }
                }
                ObjectUpdate::OutOfRange(guids) => {
                    for guid in &guids {
                        self.objects.remove(guid);
                        self.auras.forget(*guid);
                    }
                    session_events.push(SessionEvent::ObjectsRemoved(guids));
                }
                ObjectUpdate::Near(_) => {}
            }
        }
        handled(session_events, wrath_events, Vec::new())
    }
}

impl TrackedObject {
    fn from_create(object_type: ObjectType, fields: &UpdateFields) -> Self {
        let mut this = Self {
            object_type,
            values: vec![0; object_type.field_count()],
        };
        this.apply(fields);
        this
    }

    fn apply(&mut self, fields: &UpdateFields) {
        for field in &fields.values {
            if let Some(slot) = self.values.get_mut(field.index) {
                *slot = field.value;
            }
        }
    }

    fn get(&self, index: usize) -> u32 {
        self.values.get(index).copied().unwrap_or(0)
    }
}

fn handled(
    session_events: Vec<SessionEvent>,
    wrath_events: Vec<WrathEvent>,
    replies: Vec<WrathReply>,
) -> WrathDispatch {
    WrathDispatch::Handled {
        session_events,
        wrath_events,
        replies,
    }
}

fn adapt_loot(packet: ServerLootPacket) -> WrathDispatch {
    match packet {
        ServerLootPacket::Response(LootResponse::Corpse { guid, gold, items }) => handled(
            vec![SessionEvent::LootResponse {
                guid,
                loot_type: 1,
                gold,
                items: items.iter().copied().map(WrathLootItem::neutral).collect(),
            }],
            vec![WrathEvent::LootResponse { guid, gold, items }],
            Vec::new(),
        ),
        ServerLootPacket::Response(LootResponse::Error { guid, error }) => handled(
            vec![SessionEvent::LootError { guid, error }],
            Vec::new(),
            Vec::new(),
        ),
        ServerLootPacket::Release { guid } => handled(
            vec![SessionEvent::LootReleaseResponse { guid }],
            Vec::new(),
            Vec::new(),
        ),
        ServerLootPacket::Removed { slot } => handled(
            vec![SessionEvent::LootRemoved { slot }],
            Vec::new(),
            Vec::new(),
        ),
        ServerLootPacket::Money(money) => handled(
            vec![SessionEvent::LootMoneyNotify {
                amount: money.amount,
            }],
            vec![WrathEvent::LootMoneyNotify(money)],
            Vec::new(),
        ),
        ServerLootPacket::ClearMoney => {
            handled(vec![SessionEvent::LootClearMoney], Vec::new(), Vec::new())
        }
        ServerLootPacket::ItemPush(item) => handled(
            vec![SessionEvent::ItemPushResult(item.neutral())],
            vec![WrathEvent::ItemPushResult(item)],
            Vec::new(),
        ),
        ServerLootPacket::InventoryFailure(Some(failure)) => handled(
            vec![SessionEvent::WrathInventoryFailure(failure)],
            vec![WrathEvent::InventoryFailure(failure)],
            Vec::new(),
        ),
        ServerLootPacket::InventoryFailure(None) => handled(Vec::new(), Vec::new(), Vec::new()),
    }
}

fn malformed(opcode: u16, reason: &'static str) -> WrathEventError {
    WrathEventError::Malformed { opcode, reason }
}

fn le_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes.try_into().expect("length checked by caller"))
}

fn world_pose(opcode: u16, body: &[u8]) -> Result<(u32, [f32; 3], f32), WrathEventError> {
    if body.len() != 20 {
        return Err(malformed(opcode, "expected map and XYZO (20 bytes)"));
    }
    Ok((
        le_u32(&body[0..4]),
        [
            f32::from_bits(le_u32(&body[4..8])),
            f32::from_bits(le_u32(&body[8..12])),
            f32::from_bits(le_u32(&body[12..16])),
        ],
        f32::from_bits(le_u32(&body[16..20])),
    ))
}

fn vanilla_type(object_type: ObjectType) -> VanillaObjectType {
    match object_type {
        ObjectType::Object => VanillaObjectType::Object,
        ObjectType::Item => VanillaObjectType::Item,
        ObjectType::Container => VanillaObjectType::Container,
        ObjectType::Unit => VanillaObjectType::Unit,
        ObjectType::Player => VanillaObjectType::Player,
        ObjectType::GameObject => VanillaObjectType::GameObject,
        ObjectType::DynamicObject => VanillaObjectType::DynamicObject,
        ObjectType::Corpse => VanillaObjectType::Corpse,
    }
}

/// Native descriptors retain every wire word; create-zero semantics are class/layout bounded.
fn native_fields(state: &TrackedObject, created: bool) -> ObjectFields {
    let pairs: Vec<_> = state
        .values
        .iter()
        .enumerate()
        .filter(|(_, value)| **value != 0)
        .map(|(index, value)| (index as u16, *value))
        .collect();
    let fields = ObjectFields::from_native_pairs(FieldLayout::Wrath12340, &pairs);
    if created {
        fields.into_created(vanilla_type(state.object_type))
    } else {
        fields
    }
}

/// Sparse deltas preserve explicit zeros, and never manufacture neighboring descriptor words.
fn native_delta(state: &TrackedObject, fields: &UpdateFields) -> ObjectFields {
    let pairs: Vec<_> = fields
        .values
        .iter()
        .filter(|field| state.get(field.index) != field.value)
        .map(|field| (field.index as u16, field.value))
        .collect();
    ObjectFields::from_native_pairs(FieldLayout::Wrath12340, &pairs)
}

fn entity_kind(object_type: ObjectType) -> EntityKind {
    match object_type {
        ObjectType::Player => EntityKind::Player,
        ObjectType::Unit => EntityKind::Unit,
        ObjectType::GameObject => EntityKind::GameObject,
        ObjectType::DynamicObject => EntityKind::DynamicObject,
        ObjectType::Corpse => EntityKind::Corpse,
        _ => EntityKind::Other,
    }
}

fn native_display(object_type: ObjectType, fields: &ObjectFields) -> Option<u32> {
    match object_type {
        ObjectType::Unit | ObjectType::Player => fields
            .unit_displayid()
            .and_then(|id| u32::try_from(id).ok())
            .filter(|id| *id != 0),
        ObjectType::GameObject => fields
            .gameobject_displayid()
            .and_then(|id| u32::try_from(id).ok())
            .filter(|id| *id != 0),
        ObjectType::Corpse => fields.corpse_display_id(),
        _ => None,
    }
}

fn placement(movement: &MovementBlock) -> Option<([f32; 3], f32)> {
    match &movement.position {
        MovementPosition::Living(info) => Some((
            [info.position.x, info.position.y, info.position.z],
            info.position.orientation,
        )),
        MovementPosition::Positioned {
            world, orientation, ..
        } => Some((*world, *orientation)),
        MovementPosition::Stationary(position) => {
            Some(([position.x, position.y, position.z], position.orientation))
        }
        MovementPosition::None => None,
    }
}

fn project_speeds(speeds: Option<[f32; 9]>) -> Option<MoveSpeeds> {
    speeds.map(|s| MoveSpeeds {
        walk: s[0],
        run: s[1],
        run_back: s[2],
        swim: s[3],
        swim_back: s[4],
        turn_rate: s[7],
    })
}

fn project_mover(movement: &MovementBlock) -> Option<MoverState> {
    let MovementPosition::Living(info) = &movement.position else {
        return None;
    };
    // The neutral mover uses Vanilla-shaped ROOT/FALLING bits; Wrath shifted all three.
    // Transport and spline bits also changed positions and remain explicitly mapped below.
    let mut flags =
        info.flags & !(MOVEMENT_FLAG_ON_TRANSPORT | MOVEMENT_FLAG_SPLINE_ENABLED | 0x0000_3800);
    for (wrath, neutral) in [(0x0800, 0x1000), (0x1000, 0x2000), (0x2000, 0x4000)] {
        if info.flags & wrath != 0 {
            flags |= neutral;
        }
    }
    if info.flags & MOVEMENT_FLAG_ON_TRANSPORT != 0 {
        flags |= 0x0200_0000;
    }
    if info.flags & MOVEMENT_FLAG_SPLINE_ENABLED != 0 {
        flags |= 0x0040_0000;
    }
    Some(MoverState {
        flags,
        pitch: info.pitch.unwrap_or(0.0),
    })
}

fn project_transport(movement: &MovementBlock) -> Option<TransportPose> {
    let MovementPosition::Living(info) = &movement.position else {
        return None;
    };
    info.transport.as_ref().map(|transport| TransportPose {
        guid: transport.guid,
        pos: Vector3d {
            x: transport.position.x,
            y: transport.position.y,
            z: transport.position.z,
        },
        orientation: transport.position.orientation,
    })
}

fn project_spline(movement: &MovementBlock) -> Option<CreateSpline> {
    let spline = movement.spline.as_ref()?;
    Some(CreateSpline {
        path: spline.nodes.clone(),
        id: spline.id,
        time_passed_ms: spline.time_passed,
        duration_ms: spline.duration,
        // `Flying` keeps wire Z; `Catmullrom` alone selects smooth interpolation but is not flight.
        flying: spline.flags & 0x0000_2000 != 0,
        interpolation: if spline.flags & (0x0000_2000 | 0x0004_0000) != 0 {
            crate::SplineInterpolation::CatmullRom
        } else {
            crate::SplineInterpolation::Linear
        },
        cyclic: spline.flags & 0x0008_0000 != 0,
        effect: if spline.flags & 0x0020_0000 != 0 {
            Some(crate::SplineEffect::Animation {
                animation_id: spline.flags as u8,
                effect_start_time_ms: spline.effect_start_time,
            })
        } else if spline.flags & 0x0000_0800 != 0 {
            Some(crate::SplineEffect::Parabolic {
                vertical_acceleration: spline.vertical_acceleration,
                effect_start_time_ms: spline.effect_start_time,
            })
        } else {
            None
        },
        run_mode: match &movement.position {
            MovementPosition::Living(info) => Some(info.flags & 0x100 == 0),
            _ => None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wrath_gameplay::{
        CreateSpline as WrathCreateSpline, FieldValue, MovementInfo, Position, SplineFacing,
        UPDATE_FLAG_LIVING, UPDATE_FLAG_STATIONARY_POSITION,
    };

    fn fields(object_type: ObjectType, values: &[(usize, u32)]) -> UpdateFields {
        UpdateFields {
            object_type,
            field_count: values.last().map_or(0, |(i, _)| i / 32 * 32 + 32),
            values: values
                .iter()
                .map(|&(index, value)| FieldValue { index, value })
                .collect(),
        }
    }

    #[test]
    fn wrath_resources_preserve_all_seven_slots_and_sparse_raw_precision() {
        let initial = fields(
            ObjectType::Player,
            &[
                (25, 721),
                (26, 437),
                (31, 659),
                (33, 1000),
                (34, 1000),
                (39, 1250),
            ],
        );
        let mut state = TrackedObject::from_create(ObjectType::Player, &initial);
        let mut store = native_fields(&state, true);
        let powers = store.wrath_powers().unwrap();
        assert_eq!(powers[0], (721, 1000));
        assert_eq!(powers[1], (437, 1000));
        assert_eq!(powers[5], (0, 0));
        assert_eq!(powers[6], (659, 1250));
        let delta = fields(ObjectType::Player, &[(26, 439), (31, 0)]);
        store.merge(native_delta(&state, &delta));
        state.apply(&delta);
        assert_eq!(store.wrath_powers().unwrap()[1], (439, 1000));
        assert_eq!(store.wrath_powers().unwrap()[6], (0, 1250));
        assert_eq!(store.wrath_powers().unwrap()[0], (721, 1000));
        store.merge(native_fields(
            &TrackedObject::from_create(ObjectType::Player, &fields(ObjectType::Player, &[])),
            true,
        ));
        assert_eq!(store.wrath_powers(), Some([(0, 0); 7]));
        assert_eq!(state.get(26), 439);
    }

    fn living() -> MovementBlock {
        MovementBlock {
            update_flags: UPDATE_FLAG_LIVING,
            position: MovementPosition::Living(MovementInfo {
                flags: 0,
                flags2: 0,
                timestamp: 1,
                position: Position {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                    orientation: 4.0,
                },
                transport: None,
                pitch: None,
                fall_time: 0,
                jump: None,
                spline_elevation: None,
            }),
            speeds: Some([
                2.5,
                7.0,
                4.5,
                4.7,
                2.5,
                7.0,
                4.5,
                std::f32::consts::PI,
                std::f32::consts::PI,
            ]),
            spline: None,
            unknown: None,
            low_guid: None,
            target_guid: None,
            transport_progress: None,
            vehicle: None,
            rotation: None,
        }
    }

    fn raw_packed_guid(out: &mut Vec<u8>, guid: u64) {
        let bytes = guid.to_le_bytes();
        let mask = bytes
            .iter()
            .enumerate()
            .fold(0u8, |mask, (i, byte)| mask | (u8::from(*byte != 0) << i));
        out.push(mask);
        for (i, byte) in bytes.iter().enumerate() {
            if mask & (1 << i) != 0 {
                out.push(*byte);
            }
        }
    }

    fn raw_fields(out: &mut Vec<u8>, values: &[(usize, u32)]) {
        let words = values.last().map_or(0, |(index, _)| index / 32 + 1);
        out.push(words as u8);
        let mut mask = vec![0u32; words];
        for &(index, _) in values {
            mask[index / 32] |= 1 << (index % 32);
        }
        for word in mask {
            out.extend(word.to_le_bytes());
        }
        for &(_, value) in values {
            out.extend(value.to_le_bytes());
        }
    }

    fn raw_create(guid: u64, object_type: ObjectType, values: &[(usize, u32)]) -> Vec<u8> {
        let mut out = 1u32.to_le_bytes().to_vec();
        out.push(2);
        raw_packed_guid(&mut out, guid);
        out.push(object_type as u8);
        if object_type == ObjectType::Player {
            out.extend(UPDATE_FLAG_STATIONARY_POSITION.to_le_bytes());
            for value in [1.0f32, 2.0, 3.0, 4.0] {
                out.extend(value.to_le_bytes());
            }
        } else {
            out.extend(0u16.to_le_bytes());
        }
        raw_fields(&mut out, values);
        out
    }

    fn raw_delta(guid: u64, values: &[(usize, u32)]) -> Vec<u8> {
        let mut out = 1u32.to_le_bytes().to_vec();
        out.push(0);
        raw_packed_guid(&mut out, guid);
        raw_fields(&mut out, values);
        out
    }

    #[test]
    fn create_spline_catmull_rom_is_smooth_but_only_flying_keeps_wire_z() {
        let mut movement = living();
        movement.spline = Some(WrathCreateSpline {
            flags: 0x0004_0000,
            facing: SplineFacing::None,
            time_passed: 10,
            duration: 1_000,
            id: 7,
            duration_modifier: 1.0,
            next_duration_modifier: 1.0,
            vertical_acceleration: 0.0,
            effect_start_time: 0,
            nodes: vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]],
            mode: 1,
            final_destination: [4.0, 5.0, 6.0],
        });

        let smooth_ground = project_spline(&movement).unwrap();
        assert!(
            !smooth_ground.flying,
            "Catmullrom alone does not retain wire Z"
        );
        movement.spline.as_mut().unwrap().flags |= 0x0000_2000;
        assert!(project_spline(&movement).unwrap().flying);
    }

    #[test]
    fn monster_move_dispatches_into_the_shared_session_event_path() {
        let mut body = vec![0x01, 0x2a, 0]; // packed mover GUID, movement-info byte
        for value in [1.0f32, 2.0, 3.0] {
            body.extend(value.to_le_bytes());
        }
        body.extend(17u32.to_le_bytes()); // spline id
        body.push(0); // normal move
        body.extend(0u32.to_le_bytes()); // linear flags
        body.extend(1_000u32.to_le_bytes());
        body.extend(1u32.to_le_bytes()); // one absolute endpoint
        for value in [5.0f32, 2.0, 3.0] {
            body.extend(value.to_le_bytes());
        }

        let WrathDispatch::Handled { session_events, .. } = WrathEventAdapter::new()
            .dispatch(SMSG_MONSTER_MOVE, &body, 0)
            .unwrap()
        else {
            panic!("monster move must be handled")
        };
        assert!(matches!(
            session_events.as_slice(),
            [SessionEvent::MonsterMove {
                guid: 0x2a,
                spline_id: 17,
                path,
                duration_ms: 1_000,
                ..
            }] if path == &[[1.0, 2.0, 3.0], [5.0, 2.0, 3.0]]
        ));
    }

    fn minimal_item_response(entry: u32) -> Vec<u8> {
        fn word(out: &mut Vec<u8>, value: u32) {
            out.extend(value.to_le_bytes());
        }
        let mut out = Vec::new();
        word(&mut out, entry);
        for _ in 0..3 {
            word(&mut out, 0);
        }
        out.extend([0; 4]);
        for _ in 0..21 {
            word(&mut out, 0);
        }
        for _ in 0..3 {
            word(&mut out, 0);
        }
        for _ in 0..2 {
            for _ in 0..3 {
                word(&mut out, 0);
            }
        }
        for _ in 0..10 {
            word(&mut out, 0);
        }
        for _ in 0..5 {
            for value in [0, 0, 0, u32::MAX, 0, u32::MAX] {
                word(&mut out, value);
            }
        }
        word(&mut out, 0);
        out.push(0);
        for _ in 0..16 + 6 + 7 {
            word(&mut out, 0);
        }
        out
    }

    #[test]
    fn item_query_positive_and_negative_become_template_events() {
        let mut adapter = WrathEventAdapter::new();
        let WrathDispatch::Handled { session_events, .. } = adapter
            .dispatch(
                SMSG_ITEM_QUERY_SINGLE_RESPONSE,
                &minimal_item_response(117),
                0,
            )
            .unwrap()
        else {
            panic!()
        };
        assert!(matches!(
            &session_events[..],
            [SessionEvent::ItemTemplate { entry: 117, info: Some(info) },
             SessionEvent::WrathItemRollMetadata { entry: 117, metadata: Some(metadata) }]
                if info.name.is_empty() && metadata.flags2 == 0
                    && metadata.required_disenchant_skill == 0 && metadata.item_limit_category == 0
        ));

        let WrathDispatch::Handled { session_events, .. } = adapter
            .dispatch(
                SMSG_ITEM_QUERY_SINGLE_RESPONSE,
                &0x8000_270fu32.to_le_bytes(),
                0,
            )
            .unwrap()
        else {
            panic!()
        };
        assert!(matches!(
            &session_events[..],
            [SessionEvent::ItemTemplate {
                entry: 9999,
                info: None
            }, SessionEvent::WrathItemRollMetadata { entry: 9999, metadata: None }]
        ));
    }

    #[test]
    fn malformed_item_query_response_preserves_codec_error() {
        let error = WrathEventAdapter::new()
            .dispatch(SMSG_ITEM_QUERY_SINGLE_RESPONSE, &[1, 2, 3], 0)
            .unwrap_err();
        assert!(matches!(
            error,
            WrathEventError::ItemCodec {
                opcode: SMSG_ITEM_QUERY_SINGLE_RESPONSE,
                source: ItemCodecError::Truncated
            }
        ));
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn player_create_retains_native_renderer_fields_and_nine_speeds() {
        let guid = 0x42;
        let update = UpdateObject {
            objects: vec![ObjectUpdate::Create {
                update_type: 3,
                guid,
                object_type: ObjectType::Player,
                movement: living(),
                fields: fields(
                    ObjectType::Player,
                    &[
                        (4, 1.0f32.to_bits()),
                        (23, 0x0302_0101),
                        (24, 100),
                        (32, 200),
                        (54, 5),
                        (55, 7),
                        (67, 49),
                        (69, 123),
                        (74, 8),
                        (122, 2 | (17 << 24)),
                        (153, 0x0403_0201),
                        (154, 5),
                        (283, 9001),
                        (284, 12 | (34 << 16)),
                    ],
                ),
            }],
        };
        let bytes = update.to_bytes().unwrap();
        let mut adapter = WrathEventAdapter::new();
        let WrathDispatch::Handled {
            session_events,
            wrath_events,
            ..
        } = adapter.dispatch(SMSG_UPDATE_OBJECT, &bytes, 0).unwrap()
        else {
            panic!()
        };
        assert_eq!(wrath_events.len(), 1);
        let SessionEvent::ObjectCreate {
            fields,
            speeds,
            display_id,
            ..
        } = &session_events[0]
        else {
            panic!()
        };
        assert_eq!(*display_id, Some(49));
        assert_eq!(fields.unit_health(), Some(100));
        assert_eq!(fields.unit_max_health(), Some(200));
        assert_eq!(fields.unit_mount_display_id(), 123);
        assert_eq!(fields.unit_stand_state(), 8);
        assert_eq!(fields.unit_sheath_state(), Some(2));
        assert_eq!(fields.unit_shapeshift_form(), 17);
        assert_eq!(fields.player_skin(), Some(1));
        assert_eq!(fields.player_visible_item_entry(0), Some(9001));
        assert_eq!(fields.player_visible_item_enchant(0, 1), Some(34));
        assert_eq!(
            *speeds.as_ref().unwrap(),
            MoveSpeeds {
                walk: 2.5,
                run: 7.0,
                run_back: 4.5,
                swim: 4.7,
                swim_back: 2.5,
                turn_rate: std::f32::consts::PI,
            }
        );
    }

    #[test]
    fn wrath_resource_wire_deltas_are_delivered_and_zero_retained() {
        for index in [30, 31, 38, 39] {
            let guid = 42;
            let mut adapter = WrathEventAdapter::new();
            adapter
                .dispatch(
                    SMSG_UPDATE_OBJECT,
                    &raw_create(guid, ObjectType::Player, &[(index, 107)]),
                    0,
                )
                .unwrap();
            let mut store = native_fields(adapter.objects.get(&guid).unwrap(), true);
            for value in [439, 0] {
                let WrathDispatch::Handled { session_events, .. } = adapter
                    .dispatch(SMSG_UPDATE_OBJECT, &raw_delta(guid, &[(index, value)]), 0)
                    .unwrap()
                else {
                    panic!()
                };
                assert_eq!(session_events.len(), 1, "field {index}");
                let SessionEvent::ObjectValues { fields, .. } = &session_events[0] else {
                    panic!()
                };
                store.merge(fields.clone());
                let slot = if index < 33 { index - 25 } else { index - 33 };
                let pair = store.wrath_powers().unwrap()[slot];
                assert_eq!(if index < 33 { pair.0 } else { pair.1 }, value);
                let WrathDispatch::Handled { session_events, .. } = adapter
                    .dispatch(SMSG_UPDATE_OBJECT, &raw_delta(guid, &[(index, value)]), 0)
                    .unwrap()
                else {
                    panic!()
                };
                assert!(session_events.is_empty());
            }
        }
        assert!(ObjectFields::from_native_pairs(FieldLayout::Wrath12340, &[]).is_empty());
    }

    #[test]
    fn raw_item_and_container_descriptors_retain_native_fields_and_clears() {
        let item_guid = 0x4000_0000_0000_0042;
        let mut adapter = WrathEventAdapter::new();
        let create = raw_create(
            item_guid,
            ObjectType::Item,
            &[
                (10, 0x5566_7788),
                (11, 0x1122_3344),
                (14, 7),
                (16, (-3i32) as u32),
                (59, 9001),
                (60, 31),
                (61, 47),
            ],
        );
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(SMSG_UPDATE_OBJECT, &create, 0).unwrap()
        else {
            panic!()
        };
        let SessionEvent::ItemCreate { fields, .. } = &session_events[0] else {
            panic!()
        };
        assert_eq!(fields.item_stack_count(), Some(7));
        assert_eq!(fields.item_spell_charges(0), Some(-3));
        assert_eq!(fields.item_random_properties_id(), 9001);
        assert_eq!(fields.item_durability(), Some(31));
        assert_eq!(fields.item_max_durability(), Some(47));

        let clear = raw_delta(item_guid, &[(10, 0), (11, 0), (14, 0)]);
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(SMSG_UPDATE_OBJECT, &clear, 0).unwrap()
        else {
            panic!()
        };
        let SessionEvent::ObjectValues { fields, .. } = &session_events[0] else {
            panic!()
        };
        assert_eq!(fields.item_creator(), None);
        assert_eq!(fields.item_stack_count(), Some(0));

        let container_guid = 0x4000_0000_0000_0043;
        let create = raw_create(
            container_guid,
            ObjectType::Container,
            &[(64, 36), (66, 0x0506_0708), (67, 0x0102_0304), (136, 0x42)],
        );
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(SMSG_UPDATE_OBJECT, &create, 0).unwrap()
        else {
            panic!()
        };
        let SessionEvent::ItemCreate { fields, .. } = &session_events[0] else {
            panic!()
        };
        assert_eq!(fields.container_num_slots(), Some(36));
        assert_eq!(fields.container_slot(0), Some(0x0102_0304_0506_0708));
        assert_eq!(fields.container_slot(35), Some(0x42));
    }

    #[test]
    fn raw_player_inventory_groups_and_coinage_retain_wrath_extras() {
        let guid = 0x42;
        let mut adapter = WrathEventAdapter::new();
        let create = raw_create(
            guid,
            ObjectType::Player,
            &[
                (324, 0x0506_0708),
                (325, 0x0102_0304),
                (368, 0x1111),
                (369, 0x2222),
                (370, 0x1516_1718),
                (371, 0x1112_1314),
                (448, 0x2526_2728),
                (449, 0x2122_2324),
                (456, 0xaaaa),
                (457, 0xbbbb),
                (458, 0x3536_3738),
                (459, 0x3132_3334),
                (470, 0xcccc),
                (471, 0xdddd),
                (496, 0x4546_4748),
                (497, 0x4142_4344),
                (558, 0x5556_5758),
                (559, 0x5152_5354),
                (1170, 123_456),
            ],
        );
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(SMSG_UPDATE_OBJECT, &create, 0).unwrap()
        else {
            panic!()
        };
        let SessionEvent::ObjectCreate { fields, .. } = &session_events[0] else {
            panic!()
        };
        assert_eq!(fields.player_inv_slot(0), Some(0x0102_0304_0506_0708));
        assert_eq!(fields.player_inv_slot(22), Some(0x0000_2222_0000_1111));
        assert_eq!(fields.player_pack_slot(0), Some(0x1112_1314_1516_1718));
        assert_eq!(fields.player_bank_slot(23), Some(0x2122_2324_2526_2728));
        assert_eq!(fields.player_bank_bag_slot(0), Some(0x3132_3334_3536_3738));
        assert_eq!(fields.player_keyring_slot(0), Some(0x4142_4344_4546_4748));
        assert_eq!(fields.player_keyring_slot(31), Some(0x5152_5354_5556_5758));
        assert_eq!(fields.player_money(), Some(123_456));
        assert_eq!(fields.player_bank_slot(27), Some(0x0000_bbbb_0000_aaaa));
        assert_eq!(fields.player_bank_slot(28), None);
        assert_eq!(fields.player_bank_bag_slot(6), Some(0x0000_dddd_0000_cccc));
        assert_eq!(fields.player_bank_bag_slot(7), None);

        let clear = raw_delta(guid, &[(324, 0), (325, 0), (370, 0), (371, 0)]);
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(SMSG_UPDATE_OBJECT, &clear, 0).unwrap()
        else {
            panic!()
        };
        let SessionEvent::ObjectValues { fields, .. } = &session_events[0] else {
            panic!()
        };
        assert_eq!(fields.player_inv_slot(0), Some(0));
        assert_eq!(fields.player_pack_slot(0), Some(0));
    }

    #[test]
    fn sparse_value_zero_is_native_and_type_state_is_removed() {
        let guid = 0xf130_0001_0000_0001;
        let mut adapter = WrathEventAdapter::new();
        let create = UpdateObject {
            objects: vec![ObjectUpdate::Create {
                update_type: 2,
                guid,
                object_type: ObjectType::Unit,
                movement: living(),
                fields: fields(ObjectType::Unit, &[(24, 100)]),
            }],
        };
        adapter
            .dispatch(SMSG_UPDATE_OBJECT, &create.to_bytes().unwrap(), 0)
            .unwrap();
        let delta = UpdateObject {
            objects: vec![
                ObjectUpdate::Values {
                    guid,
                    fields: fields(ObjectType::Unit, &[(24, 0)]),
                },
                ObjectUpdate::OutOfRange(vec![guid]),
            ],
        };
        let WrathDispatch::Handled { session_events, .. } = adapter
            .dispatch(SMSG_UPDATE_OBJECT, &delta.to_bytes().unwrap(), 0)
            .unwrap()
        else {
            panic!()
        };
        let SessionEvent::ObjectValues { fields, .. } = &session_events[0] else {
            panic!()
        };
        assert_eq!(fields.unit_health(), Some(0));
        assert_eq!(adapter.object_type(guid), None);
    }

    #[test]
    fn create_defaults_unchanged_delta_recreate_and_removal_follow_lifecycle() {
        let guid = 0xf130_0001_0000_0001;
        let mut adapter = WrathEventAdapter::new();
        let update = UpdateObject {
            objects: vec![
                ObjectUpdate::Create {
                    update_type: 2,
                    guid,
                    object_type: ObjectType::Unit,
                    movement: living(),
                    fields: fields(ObjectType::Unit, &[(24, 100), (67, 49)]),
                },
                ObjectUpdate::Values {
                    guid,
                    fields: fields(ObjectType::Unit, &[(24, 100)]),
                },
                ObjectUpdate::Create {
                    update_type: 2,
                    guid,
                    object_type: ObjectType::Unit,
                    movement: living(),
                    fields: fields(ObjectType::Unit, &[(67, 77)]),
                },
                ObjectUpdate::OutOfRange(vec![guid]),
            ],
        };
        let WrathDispatch::Handled { session_events, .. } = adapter
            .dispatch(SMSG_UPDATE_OBJECT, &update.to_bytes().unwrap(), 0)
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(session_events.len(), 3, "unchanged delta emits no edge");
        let SessionEvent::ObjectCreate { fields, .. } = &session_events[0] else {
            panic!()
        };
        assert_eq!(fields.unit_health(), Some(100));
        let SessionEvent::ObjectCreate { fields, .. } = &session_events[1] else {
            panic!()
        };
        assert_eq!(
            fields.unit_health(),
            Some(0),
            "recreate resets omitted fields"
        );
        assert_eq!(fields.unit_displayid(), Some(77));
        assert!(matches!(
            &session_events[2],
            SessionEvent::ObjectsRemoved(guids) if guids == &[guid]
        ));
        assert_eq!(adapter.object_type(guid), None);
    }

    #[test]
    fn movement_marked_self_is_rejected_instead_of_fabricated() {
        let guid = 0x42;
        let update = UpdateObject {
            objects: vec![ObjectUpdate::Movement {
                guid,
                movement: MovementBlock {
                    update_flags: UPDATE_FLAG_SELF | UPDATE_FLAG_LIVING,
                    ..living()
                },
            }],
        };
        assert!(matches!(
            WrathEventAdapter::new().dispatch(SMSG_UPDATE_OBJECT, &update.to_bytes().unwrap(), 0),
            Err(WrathEventError::Malformed {
                reason: "self movement update has no lossless session-event representation",
                ..
            })
        ));
    }

    #[test]
    fn bootstrap_and_time_sync_are_strict() {
        let mut adapter = WrathEventAdapter::new();
        let mut world = 1u32.to_le_bytes().to_vec();
        for value in [2.0f32, 3.0, 4.0, 5.0] {
            world.extend(value.to_le_bytes());
        }
        let WrathDispatch::Handled { session_events, .. } = adapter
            .dispatch(SMSG_LOGIN_VERIFY_WORLD, &world, 0)
            .unwrap()
        else {
            panic!()
        };
        assert!(matches!(
            session_events[0],
            SessionEvent::Worldport {
                needs_ack: false,
                ..
            }
        ));
        let WrathDispatch::Handled { replies, .. } = adapter
            .dispatch(SMSG_TIME_SYNC_REQ, &7u32.to_le_bytes(), 99)
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(
            replies[0].body,
            [7u32.to_le_bytes(), 99u32.to_le_bytes()].concat()
        );
        assert!(adapter.dispatch(SMSG_TIME_SYNC_REQ, &[0; 3], 0).is_err());
        assert!(matches!(
            adapter.dispatch(SMSG_WARDEN_DATA, &[], 0),
            Err(WrathEventError::WardenRequired)
        ));
    }
}

#[cfg(test)]
mod native_descriptor_tests {
    use super::*;

    #[test]
    fn npc_flags_retain_services_absent_from_vanilla() {
        let fields = ObjectFields::from_native_pairs(FieldLayout::Wrath12340, &[(82, 0x1100_1081)]);
        assert_eq!(fields.unit_npc_flags(), 0x1100_1081);
    }

    #[test]
    fn final_quest_slot_preserves_unclamped_counters_and_timer() {
        let fields = ObjectFields::from_native_pairs(
            FieldLayout::Wrath12340,
            &[
                (278, 783),
                (279, 1),
                (280, 3 | (65535 << 16)),
                (281, 70 << 16),
                (282, 12345),
            ],
        );
        let quest = fields.player_quest_log(24).unwrap();
        assert_eq!(quest.quest_id, 783);
        assert_eq!(quest.counters, [3, 65535, 0, 70]);
        assert_eq!(quest.state, 1);
        assert_eq!(quest.timer, 12345);
        assert_eq!(fields.player_quest_log(25), None);
    }
}
