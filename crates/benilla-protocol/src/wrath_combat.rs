//! Native targeting and melee wire codecs for Wrath 3.3.5a, build 12340.
//!
//! Layouts and conditional tails follow AzerothCore's `CombatHandler.cpp`,
//! `MiscHandler.cpp`, `CombatPackets.cpp`, and `Unit::SendAttackStateUpdate`. Wrath's swing
//! report is intentionally kept separate from the Vanilla codec: it adds overkill, carries
//! school masks, uses a `u8` target state, and makes absorb/resist/block/rage arrays conditional.

use std::fmt;

pub const CMSG_SET_SELECTION: u32 = 0x013d;
pub const CMSG_STAND_STATE_CHANGE: u32 = 0x0101;
pub const CMSG_ATTACK_SWING: u32 = 0x0141;
pub const CMSG_ATTACK_STOP: u32 = 0x0142;
pub const CMSG_SET_SHEATHED: u32 = 0x01e0;
// Canonical opcode spellings used by AzerothCore's build-12340 table.
pub const CMSG_ATTACKSWING: u32 = CMSG_ATTACK_SWING;
pub const CMSG_ATTACKSTOP: u32 = CMSG_ATTACK_STOP;
pub const CMSG_SETSHEATHED: u32 = CMSG_SET_SHEATHED;

pub const SMSG_AI_REACTION: u16 = 0x013c;
pub const SMSG_ATTACK_START: u16 = 0x0143;
pub const SMSG_ATTACK_STOP: u16 = 0x0144;
pub const SMSG_ATTACK_SWING_NOT_IN_RANGE: u16 = 0x0145;
pub const SMSG_ATTACK_SWING_BAD_FACING: u16 = 0x0146;
pub const SMSG_ATTACK_SWING_DEAD_TARGET: u16 = 0x0148;
pub const SMSG_ATTACK_SWING_CANT_ATTACK: u16 = 0x0149;
pub const SMSG_ATTACKER_STATE_UPDATE: u16 = 0x014a;
pub const SMSG_CANCEL_COMBAT: u16 = 0x014e;
pub const SMSG_STAND_STATE_UPDATE: u16 = 0x029d;
pub const SMSG_ATTACKSTART: u16 = SMSG_ATTACK_START;
pub const SMSG_ATTACKSTOP: u16 = SMSG_ATTACK_STOP;
pub const SMSG_ATTACKERSTATEUPDATE: u16 = SMSG_ATTACKER_STATE_UPDATE;

pub const HITINFO_UNK1: u32 = 0x0000_0001;
pub const HITINFO_AFFECTS_VICTIM: u32 = 0x0000_0002;
pub const HITINFO_OFFHAND: u32 = 0x0000_0004;
pub const HITINFO_UNK2: u32 = 0x0000_0008;
pub const HITINFO_MISS: u32 = 0x0000_0010;
pub const HITINFO_FULL_ABSORB: u32 = 0x0000_0020;
pub const HITINFO_PARTIAL_ABSORB: u32 = 0x0000_0040;
pub const HITINFO_FULL_RESIST: u32 = 0x0000_0080;
pub const HITINFO_PARTIAL_RESIST: u32 = 0x0000_0100;
pub const HITINFO_CRITICAL_HIT: u32 = 0x0000_0200;
pub const HITINFO_UNK10: u32 = 0x0000_0400;
pub const HITINFO_UNK11: u32 = 0x0000_0800;
pub const HITINFO_UNK12: u32 = 0x0000_1000;
pub const HITINFO_BLOCK: u32 = 0x0000_2000;
pub const HITINFO_UNK14: u32 = 0x0000_4000;
pub const HITINFO_UNK15: u32 = 0x0000_8000;
pub const HITINFO_GLANCING: u32 = 0x0001_0000;
pub const HITINFO_CRUSHING: u32 = 0x0002_0000;
pub const HITINFO_NO_ANIMATION: u32 = 0x0004_0000;
pub const HITINFO_UNK19: u32 = 0x0008_0000;
pub const HITINFO_UNK20: u32 = 0x0010_0000;
pub const HITINFO_SWING_NO_HIT_SOUND: u32 = 0x0020_0000;
pub const HITINFO_UNK22: u32 = 0x0040_0000;
pub const HITINFO_RAGE_GAIN: u32 = 0x0080_0000;
pub const HITINFO_FAKE_DAMAGE: u32 = 0x0100_0000;

const KNOWN_HIT_INFO: u32 = 0x01ff_ffff;
const ABSORB_BITS: u32 = HITINFO_FULL_ABSORB | HITINFO_PARTIAL_ABSORB;
const RESIST_BITS: u32 = HITINFO_FULL_RESIST | HITINFO_PARTIAL_RESIST;
const MAX_DAMAGE_BLOCKS: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CombatCodecError {
    Truncated,
    TrailingBytes(usize),
    UnsupportedOpcode(u16),
    InvalidStandState(u32),
    UnsupportedHitInfo(u32),
    UnsupportedDamageCount(u8),
    UnsupportedTargetState(u8),
    UnsupportedAttackerState(u32),
}

impl fmt::Display for CombatCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(f, "truncated Wrath combat packet"),
            Self::TrailingBytes(count) => {
                write!(f, "{count} trailing byte(s) in Wrath combat packet")
            }
            Self::UnsupportedOpcode(opcode) => {
                write!(f, "unsupported Wrath combat opcode {opcode:#05x}")
            }
            Self::InvalidStandState(state) => write!(f, "invalid Wrath stand state {state}"),
            Self::UnsupportedHitInfo(flags) => write!(f, "unsupported Wrath HitInfo {flags:#010x}"),
            Self::UnsupportedDamageCount(count) => {
                write!(f, "unsupported Wrath melee damage count {count}")
            }
            Self::UnsupportedTargetState(state) => {
                write!(f, "unsupported Wrath melee target state {state}")
            }
            Self::UnsupportedAttackerState(state) => {
                write!(f, "unsupported Wrath melee attacker state {state}")
            }
        }
    }
}

impl std::error::Error for CombatCodecError {}

pub type Result<T> = std::result::Result<T, CombatCodecError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheathState {
    Unarmed,
    Melee,
    Ranged,
}

impl SheathState {
    fn wire(self) -> u32 {
        match self {
            Self::Unarmed => 0,
            Self::Melee => 1,
            Self::Ranged => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackSwingRefusal {
    NotInRange,
    BadFacing,
    DeadTarget,
    CantAttack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackStart {
    pub attacker: u64,
    pub victim: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackStop {
    pub attacker: u64,
    pub victim: u64,
    pub now_dead: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiReaction {
    pub unit: u64,
    pub reaction: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeleeDamage {
    /// `SpellSchoolMask`, not a school index.
    pub school_mask: u32,
    /// The float copy written by AzerothCore before the integer copy.
    pub damage_float: f32,
    pub damage: u32,
    pub absorb: Option<u32>,
    pub resist: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttackerState {
    pub hit_info: u32,
    pub attacker: u64,
    pub victim: u64,
    pub damage: u32,
    pub overkill: u32,
    pub damages: Vec<MeleeDamage>,
    /// AzerothCore's build-12340 `VictimState`, validated as 0 through 8.
    pub target_state: u8,
    /// AzerothCore currently writes zero; nonzero values are rejected rather than guessed.
    pub attacker_state: u32,
    pub melee_spell_id: u32,
    pub blocked: Option<u32>,
    pub rage_gain: Option<u32>,
}

/// `CMSG_SET_SELECTION` and `CMSG_ATTACKSWING` both carry one unpacked `ObjectGuid`.
pub fn encode_guid(guid: u64) -> Vec<u8> {
    guid.to_le_bytes().to_vec()
}

pub fn encode_set_selection(guid: u64) -> Vec<u8> {
    encode_guid(guid)
}

pub fn encode_attack_swing(victim: u64) -> Vec<u8> {
    encode_guid(victim)
}

/// `CMSG_ATTACKSTOP` has no fields in `CombatHandler.cpp`.
pub fn encode_attack_stop() -> Vec<u8> {
    Vec::new()
}

/// `CMSG_SET_SHEATHED` is one validated `u32`; AzerothCore admits exactly states 0, 1, and 2.
pub fn encode_set_sheathed(state: SheathState) -> Vec<u8> {
    state.wire().to_le_bytes().to_vec()
}

/// `MiscHandler::HandleStandStateChangeOpcode` accepts stand, sit, sleep and kneel only.
pub fn encode_stand_state_change(state: u32) -> Result<Vec<u8>> {
    if !matches!(state, 0 | 1 | 3 | 8) {
        return Err(CombatCodecError::InvalidStandState(state));
    }
    Ok(state.to_le_bytes().to_vec())
}

/// `Unit::SetStandState` sends the player's authoritative one-byte state.
pub fn decode_stand_state_update(body: &[u8]) -> Result<u8> {
    let mut input = Input::new(body);
    let state = input.u8()?;
    if state > 9 {
        return Err(CombatCodecError::InvalidStandState(u32::from(state)));
    }
    input.finish()?;
    Ok(state)
}

pub fn decode_cancel_combat(body: &[u8]) -> Result<()> {
    Input::new(body).finish()
}

pub fn decode_attack_start(body: &[u8]) -> Result<AttackStart> {
    let mut input = Input::new(body);
    let value = AttackStart {
        attacker: input.u64()?,
        victim: input.u64()?,
    };
    input.finish()?;
    Ok(value)
}

pub fn decode_attack_stop(body: &[u8]) -> Result<AttackStop> {
    let mut input = Input::new(body);
    let value = AttackStop {
        attacker: input.packed_guid()?,
        victim: input.packed_guid()?,
        now_dead: input.u32()? != 0,
    };
    input.finish()?;
    Ok(value)
}

pub fn decode_ai_reaction(body: &[u8]) -> Result<AiReaction> {
    let mut input = Input::new(body);
    let value = AiReaction {
        unit: input.u64()?,
        reaction: input.u32()?,
    };
    input.finish()?;
    Ok(value)
}

pub fn decode_attack_swing_refusal(opcode: u16, body: &[u8]) -> Result<AttackSwingRefusal> {
    if !body.is_empty() {
        return Err(CombatCodecError::TrailingBytes(body.len()));
    }
    match opcode {
        SMSG_ATTACK_SWING_NOT_IN_RANGE => Ok(AttackSwingRefusal::NotInRange),
        SMSG_ATTACK_SWING_BAD_FACING => Ok(AttackSwingRefusal::BadFacing),
        SMSG_ATTACK_SWING_DEAD_TARGET => Ok(AttackSwingRefusal::DeadTarget),
        SMSG_ATTACK_SWING_CANT_ATTACK => Ok(AttackSwingRefusal::CantAttack),
        _ => Err(CombatCodecError::UnsupportedOpcode(opcode)),
    }
}

pub fn decode_attacker_state(body: &[u8]) -> Result<AttackerState> {
    let mut input = Input::new(body);
    let hit_info = input.u32()?;
    // AzerothCore's only extra conditional structure is HITINFO_UNK1's undocumented 48-byte
    // debug tail. Reject it, and bits outside the referenced build's HitInfo enum, rather than
    // assigning semantics to fields not emitted in ordinary server combat.
    if hit_info & (HITINFO_UNK1 | !KNOWN_HIT_INFO) != 0 {
        return Err(CombatCodecError::UnsupportedHitInfo(hit_info));
    }
    let attacker = input.packed_guid()?;
    let victim = input.packed_guid()?;
    let damage = input.u32()?;
    let overkill = input.u32()?;
    let count = input.u8()?;
    if !(1..=MAX_DAMAGE_BLOCKS).contains(&count) {
        return Err(CombatCodecError::UnsupportedDamageCount(count));
    }

    let mut damages = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        damages.push(MeleeDamage {
            school_mask: input.u32()?,
            damage_float: input.f32()?,
            damage: input.u32()?,
            absorb: None,
            resist: None,
        });
    }
    if hit_info & ABSORB_BITS != 0 {
        for entry in &mut damages {
            entry.absorb = Some(input.u32()?);
        }
    }
    if hit_info & RESIST_BITS != 0 {
        for entry in &mut damages {
            entry.resist = Some(input.u32()?);
        }
    }

    let target_state = input.u8()?;
    if target_state > 8 {
        return Err(CombatCodecError::UnsupportedTargetState(target_state));
    }
    let attacker_state = input.u32()?;
    if attacker_state != 0 {
        return Err(CombatCodecError::UnsupportedAttackerState(attacker_state));
    }
    let melee_spell_id = input.u32()?;
    let blocked = (hit_info & HITINFO_BLOCK != 0)
        .then(|| input.u32())
        .transpose()?;
    let rage_gain = (hit_info & HITINFO_RAGE_GAIN != 0)
        .then(|| input.u32())
        .transpose()?;
    input.finish()?;

    Ok(AttackerState {
        hit_info,
        attacker,
        victim,
        damage,
        overkill,
        damages,
        target_state,
        attacker_state,
        melee_spell_id,
        blocked,
        rage_gain,
    })
}

struct Input<'a> {
    remaining: &'a [u8],
}

impl<'a> Input<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        if self.remaining.len() < count {
            return Err(CombatCodecError::Truncated);
        }
        let (head, tail) = self.remaining.split_at(count);
        self.remaining = tail;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four-byte slice"),
        ))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("eight-byte slice"),
        ))
    }

    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn packed_guid(&mut self) -> Result<u64> {
        let mask = self.u8()?;
        let mut guid = 0u64;
        for byte in 0..8 {
            if mask & (1 << byte) != 0 {
                guid |= u64::from(self.u8()?) << (byte * 8);
            }
        }
        Ok(guid)
    }

    fn finish(self) -> Result<()> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(CombatCodecError::TrailingBytes(self.remaining.len()))
        }
    }
}
