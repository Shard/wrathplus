//! The unit-token resolver `0x515970`, token to guid, over the state it reads, which the app pushes
//! as [`UnitGuids`]: a base (`player`, `pet`, `target`, `partyN`, `partypetN`, `raidN`,
//! `raidpetN`, `mouseover`, `npc`, and Wrath's `focus`, `arenaN`, `arenapetN`), then any number of `target` hops, each off the current unit's
//! `UNIT_FIELD_TARGET`. Every compare folds ASCII case (`SStrCmpI 0x64a4c0`, first at `0x5159b4`).

use std::collections::HashMap;

/// What the resolver reads: the guid globals, the group tables and, for each unit a token can
/// reach, whether it is held and whom it targets. A zero guid is nobody.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnitGuids {
    /// The local player's guid; 0 without the player object, when `player` and `pet` answer nil
    /// (`0x5159c9`, `0x515a58`).
    pub player: u64,
    /// The pet: `UNIT_FIELD_CHARM`, else `UNIT_FIELD_SUMMON` (`0x515a64`).
    pub pet: u64,
    /// The target global `[0xb4e2d8]`.
    pub target: u64,
    /// The mouseover global `[0xb4e2c8]`, read only while it is a held unit (`0x515bd2`).
    pub mouseover: u64,
    /// The interaction NPC `[0xb4e2d0]`.
    pub npc: u64,
    /// `partyN`'s guid, index N-1 (`0x4e81a0`).
    pub party: [u64; 4],
    /// `partypetN`'s guid, index N-1 (`0x4e81d0`).
    pub party_pets: [u64; 4],
    /// `raidN`'s guid, one per roster row; a row past the end is nobody (`0x491940`).
    pub raid: Vec<u64>,
    /// `raidpetN`'s guid, row for row with [`Self::raid`] (`0x491960`).
    pub raid_pets: Vec<u64>,
    /// Wrath's focus unit; 0 for none.
    pub focus: u64,
    /// `arenaN`'s opponent, slot N-1 (`0xbe9f48`); the app's arena feed owns it, not this push.
    pub arena: [u64; 5],
    /// `arenapetN`'s stored pet, slot N-1 (`0xbe9ef8`), the arena feed's too.
    pub arena_pets: [u64; 5],
    /// Whether the resolver reads Wrath's tokens (`focus`, `vehicle`, `bossN`, `arenaN`,
    /// `arenapetN`). The VM sets it from its dialect; the app's pushes leave it alone.
    pub wrath: bool,
    /// Each held unit a token can reach, to its `UNIT_FIELD_TARGET` (0 for none): the object
    /// manager's lookup with the unit typemask (`0x468460`, `ecx = 8`) and the descriptor read a
    /// `target` hop makes (`0x515a0f`-`0x515a25`). A guid missing here is not held.
    pub held: HashMap<u64, u64>,
}

impl UnitGuids {
    /// Every token naming `guid`, in the order build 12340's unit-event fan-out lists them
    /// (`0x60bb70`, which `0x60bf60`/`0x60bf10` walk for each `UNIT_*` event): `player`, `pet`,
    /// the first `partyN`/`partypetN` match, the first `raidN`/`raidpetN` match, `target`, `focus`,
    /// `mouseover`. Not built (labelled): `vehicle`, `arenaN`/`arenapetN` and `bossN`, which these
    /// inputs do not carry. A zero guid names nothing.
    pub fn event_tokens(&self, guid: u64) -> Vec<String> {
        let mut out = Vec::new();
        if guid == 0 {
            return out;
        }
        if self.player == guid {
            out.push("player".into());
        }
        if self.pet == guid {
            out.push("pet".into());
        }
        for i in 0..4 {
            if self.party[i] == guid {
                out.push(format!("party{}", i + 1));
                break;
            }
            if self.party_pets[i] == guid {
                out.push(format!("partypet{}", i + 1));
                break;
            }
        }
        for (i, (&raid, &pet)) in self
            .raid
            .iter()
            .zip(self.raid_pets.iter().chain(std::iter::repeat(&0)))
            .enumerate()
        {
            if raid == guid {
                out.push(format!("raid{}", i + 1));
                break;
            }
            if pet == guid {
                out.push(format!("raidpet{}", i + 1));
                break;
            }
        }
        // `0x60bb70`: the arena slots after the raid rows, over `GetNumArenaOpponents`' count.
        for i in 0..self.arena_count() {
            if self.arena[i] == guid {
                out.push(format!("arena{}", i + 1));
                break;
            }
            if self.arena_pets[i] == guid {
                out.push(format!("arenapet{}", i + 1));
                break;
            }
        }
        for (token, slot) in [
            ("target", self.target),
            ("focus", self.focus),
            ("mouseover", self.mouseover),
        ] {
            if slot == guid {
                out.push(token.into());
            }
        }
        out
    }
}

impl crate::script::UiScript {
    /// [`UnitGuids::event_tokens`] over the resolver inputs the app last pushed.
    pub fn unit_event_tokens(&self, guid: u64) -> Vec<String> {
        self.model_ref().unit_guids.event_tokens(guid)
    }

    /// The arena feed's slots: `arenaN` and `arenapetN`, slot N-1, 0 for none.
    pub fn set_arena_units(&mut self, arena: [u64; 5], pets: [u64; 5]) {
        let mut model = self.model_mut();
        model.unit_guids.arena = arena;
        model.unit_guids.arena_pets = pets;
    }
}

/// A token's base, the resolver's compares in its order (`partypet` before `party`, `raidpet`
/// before `raid`); `npc` is tested apart, as its one full-string compare.
#[derive(Clone, Copy)]
enum Base {
    Player,
    Pet,
    Target,
    PartyPet,
    Party,
    RaidPet,
    Raid,
    Mouseover,
}

/// Each prefix and its base, in the resolver's order; a prefix test compares `strlen(literal)`
/// bytes, so `"playerfoo"` matches `player`.
const PREFIXES: [(&str, Base); 8] = [
    ("player", Base::Player),
    ("pet", Base::Pet),
    ("target", Base::Target),
    ("partypet", Base::PartyPet),
    ("party", Base::Party),
    ("raidpet", Base::RaidPet),
    ("raid", Base::Raid),
    ("mouseover", Base::Mouseover),
];

/// The unit a token starts from, before any `target` hop. A numbered base carries its 0-based
/// row, [`index_of`]'s: a table lookup that fails on the reserved `u32::MAX` of a missing or zero
/// number names nobody.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitBase {
    Player,
    Pet,
    Target,
    Mouseover,
    /// The interaction NPC; an exact compare, so it takes no `target` hop (`0x515bec`).
    Npc,
    Party(u32),
    PartyPet(u32),
    Raid(u32),
    RaidPet(u32),
    /// Wrath's bases.
    Focus,
    Vehicle,
    Boss(u32),
    Arena(u32),
    ArenaPet(u32),
}

/// Wrath's additional bases, after the 1.12 compares; `arenapet` before `arena`.
#[derive(Clone, Copy)]
enum WrathBase {
    Focus,
    Vehicle,
    Boss,
    ArenaPet,
    Arena,
}

const WRATH_PREFIXES: [(&str, WrathBase); 5] = [
    ("focus", WrathBase::Focus),
    ("vehicle", WrathBase::Vehicle),
    ("boss", WrathBase::Boss),
    ("arenapet", WrathBase::ArenaPet),
    ("arena", WrathBase::Arena),
];

/// What `0x515970` makes of a token's text, before it reads any state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitTokenParse {
    /// None of the nine compares match: the reference raises `Unknown unit name` (`0x515c14`).
    Unknown,
    /// Recognised, and names nobody whatever the state: the empty token (`0x51599d`), or text
    /// after the base, and after each hop, that is not `target` (`0x515c2d`).
    Nobody,
    /// A base and the `target` hops after it, each off the current unit's `UNIT_FIELD_TARGET`
    /// (`0x515a1c`-`0x515a25`); the loop has no depth limit.
    Unit { base: UnitBase, hops: usize },
}

/// `_strnicmp(s, lit, strlen(lit)) == 0`: ASCII fold only, bytes of 0x80 and up exact.
fn has_prefix(s: &[u8], lit: &str) -> bool {
    s.len() >= lit.len() && s[..lit.len()].eq_ignore_ascii_case(lit.as_bytes())
}

/// The first prefix the token matches, with its length.
fn base_of(token: &[u8]) -> Option<(Base, usize)> {
    PREFIXES
        .iter()
        .find(|(p, _)| has_prefix(token, p))
        .map(|&(p, b)| (b, p.len()))
}

/// Whether the resolver recognises the token, not whether it names a unit: `"party5"` solo is
/// recognised and answers nil, and only a token none of its nine compares match raises.
pub(crate) fn token_recognised(token: &str, wrath: bool) -> bool {
    token.eq_ignore_ascii_case("npc")
        || (wrath && token.eq_ignore_ascii_case("questnpc"))
        || base_of(token.as_bytes()).is_some()
        || (wrath && wrath_base_of(token.as_bytes()).is_some())
}

fn wrath_base_of(token: &[u8]) -> Option<(WrathBase, usize)> {
    WRATH_PREFIXES
        .iter()
        .find(|(p, _)| has_prefix(token, p))
        .map(|&(p, b)| (b, p.len()))
}

/// The inline base-10 parse after `party`, `partypet`, `raid` and `raidpet`, then `dec` to a
/// 0-based index (`0x515ab1`-`0x515acc`): digits accumulate with 32-bit wrap, and a missing
/// number leaves 0, whose index `0xffffffff` every bound rejects.
fn index_of(s: &[u8]) -> (u32, usize) {
    let digits = s.iter().take_while(|b| b.is_ascii_digit()).count();
    let n = s[..digits].iter().fold(0u32, |acc, &b| {
        acc.wrapping_mul(10).wrapping_add(u32::from(b - b'0'))
    });
    (n.wrapping_sub(1), digits)
}

/// Read `token`'s text as `0x515970` does, before any state: the base by the compares in their
/// order, a numbered base's row, then `target` hops to the end of the text. `"party1target"` is
/// party1 and one hop, `"targettarget"` the target and one, and `"npctarget"` is none of the nine
/// compares, as `npc` is one full-string compare.
pub fn parse_unit_token(token: &str) -> UnitTokenParse {
    parse_unit_token_in(token, false)
}

/// [`parse_unit_token`] in a dialect: `wrath` adds Wrath's bases after the 1.12 compares.
pub fn parse_unit_token_in(token: &str, wrath: bool) -> UnitTokenParse {
    let bytes = token.as_bytes();
    if bytes.is_empty() {
        return UnitTokenParse::Nobody;
    }
    // 2.0's quest frame names the conversation NPC `questnpc` (`QuestFrame.lua`), the unit 1.12
    // calls `npc`.
    if wrath && token.eq_ignore_ascii_case("questnpc") {
        return UnitTokenParse::Unit {
            base: UnitBase::Npc,
            hops: 0,
        };
    }
    if wrath && base_of(bytes).is_none() {
        if let Some((base, len)) = wrath_base_of(bytes) {
            let mut rest = &bytes[len..];
            let mut numbered = |make: fn(u32) -> UnitBase| {
                let (row, digits) = index_of(rest);
                rest = &rest[digits..];
                make(row)
            };
            let base = match base {
                WrathBase::Focus => UnitBase::Focus,
                WrathBase::Vehicle => UnitBase::Vehicle,
                WrathBase::Boss => numbered(UnitBase::Boss),
                WrathBase::ArenaPet => numbered(UnitBase::ArenaPet),
                WrathBase::Arena => numbered(UnitBase::Arena),
            };
            return target_hops(base, rest);
        }
    }
    let Some((base, len)) = base_of(bytes) else {
        return if token.eq_ignore_ascii_case("npc") {
            UnitTokenParse::Unit {
                base: UnitBase::Npc,
                hops: 0,
            }
        } else {
            UnitTokenParse::Unknown
        };
    };
    let mut rest = &bytes[len..];
    let mut numbered = |make: fn(u32) -> UnitBase| {
        let (row, digits) = index_of(rest);
        rest = &rest[digits..];
        make(row)
    };
    let base = match base {
        Base::Player => UnitBase::Player,
        Base::Pet => UnitBase::Pet,
        Base::Target => UnitBase::Target,
        Base::Mouseover => UnitBase::Mouseover,
        Base::PartyPet => numbered(UnitBase::PartyPet),
        Base::Party => numbered(UnitBase::Party),
        Base::RaidPet => numbered(UnitBase::RaidPet),
        Base::Raid => numbered(UnitBase::Raid),
    };
    target_hops(base, rest)
}

/// The `target` hops after a base, to the end of the text.
fn target_hops(base: UnitBase, mut rest: &[u8]) -> UnitTokenParse {
    let mut hops = 0;
    while !rest.is_empty() {
        if !has_prefix(rest, "target") {
            return UnitTokenParse::Nobody;
        }
        rest = &rest[6..];
        hops += 1;
    }
    UnitTokenParse::Unit { base, hops }
}

impl UnitGuids {
    /// The guid `token` names, or `None` for nobody; `Err` for a token none of the nine compares
    /// match, where the reference raises `Unknown unit name: %s` (`0x515c14`). An empty token is
    /// nobody (`0x51599d`).
    pub(crate) fn resolve(&self, token: &str) -> Result<Option<u64>, ()> {
        match parse_unit_token_in(token, self.wrath) {
            UnitTokenParse::Unknown => Err(()),
            UnitTokenParse::Nobody => Ok(None),
            UnitTokenParse::Unit { base, hops } => Ok(self.resolve_unit(base, hops)),
        }
    }

    /// The guid a parsed token names: its base's guid, then each `target` hop. `None` is nobody.
    pub(crate) fn resolve_unit(&self, base: UnitBase, hops: usize) -> Option<u64> {
        // The indexed bases: the table's row for the parsed number.
        let at = |table: &[u64], row: u32| {
            usize::try_from(row)
                .ok()
                .and_then(|row| table.get(row))
                .copied()
                .unwrap_or(0)
        };
        let guid = match base {
            UnitBase::Player => self.player,
            UnitBase::Pet if self.player == 0 => return None,
            UnitBase::Pet => self.pet,
            UnitBase::Target => self.target,
            UnitBase::Npc => self.npc,
            // Deviation: `0x4e81d0` has no bound and reads past its four-entry table, which is
            // stale memory, not a mechanism; past it, nobody.
            UnitBase::PartyPet(row) => at(&self.party_pets, row),
            // Capped at 4 (`0x4e81b0`).
            UnitBase::Party(row) => at(&self.party, row),
            // Capped at the roster count (`0x491940`, `0x491960`).
            UnitBase::RaidPet(row) => at(&self.raid_pets, row),
            UnitBase::Raid(row) => at(&self.raid, row),
            UnitBase::Mouseover if !self.held.contains_key(&self.mouseover) => return None,
            UnitBase::Mouseover => self.mouseover,
            UnitBase::Focus => self.focus,
            UnitBase::Arena(row) => at(&self.arena, row),
            UnitBase::ArenaPet(row) => at(&self.arena_pets, row),
            UnitBase::Vehicle | UnitBase::Boss(_) => 0,
        };
        self.follow(guid, hops)
    }

    /// The guids a `target` hop can end on: each held unit's target, when it names one. `held`
    /// holds the bases' units and each unit a hop reaches from them, so a token with a hop names
    /// one of these or nobody, and a per-guid feed over them covers the chains a script can spell.
    /// A guid repeats when two units share a target, and the last hop's unit need not be held: the
    /// lookup that finds its object is the reader's.
    pub(crate) fn chain_ends(&self) -> impl Iterator<Item = u64> + '_ {
        self.held.values().copied().filter(|&guid| guid != 0)
    }

    /// `0x549630`: the slots before the first holding neither an opponent nor a pet, which
    /// `GetNumArenaOpponents` (`0x54a2c0`) answers.
    pub(crate) fn arena_count(&self) -> usize {
        (0..5)
            .take_while(|&i| self.arena[i] != 0 || self.arena_pets[i] != 0)
            .count()
    }

    /// `GetNumArenaOpponents()` over the pushed slots.
    pub(crate) fn arena_lua(lua: &mlua::Lua, (): ()) -> mlua::Result<usize> {
        let model = lua
            .app_data_ref::<crate::script::Model>()
            .expect("model app_data");
        Ok(model.unit_guids.arena_count())
    }

    /// `next`, keeping what neither the app's unit push nor its zero default may move: the VM's
    /// dialect flag and the arena feed's slots.
    pub(crate) fn with_kept(&self, next: &UnitGuids) -> UnitGuids {
        UnitGuids {
            wrath: self.wrath,
            arena: self.arena,
            arena_pets: self.arena_pets,
            ..next.clone()
        }
    }

    /// [`Self::resolve`] with the raise as the Lua error it is.
    pub(crate) fn guid_of(&self, token: &str) -> mlua::Result<Option<u64>> {
        self.resolve(token)
            .map_err(|()| mlua::Error::runtime(format!("Unknown unit name: {token}")))
    }

    /// The `target` loop (`0x5159d3`-`0x515a2c`): nobody stays nobody, and each hop goes to the held
    /// unit's `UNIT_FIELD_TARGET`; a hop off a unit not held is a quiet nobody (`0x515a16`).
    fn follow(&self, mut guid: u64, hops: usize) -> Option<u64> {
        for _ in 0..hops {
            if guid == 0 {
                return None;
            }
            guid = *self.held.get(&guid)?;
        }
        Some(guid).filter(|&g| g != 0)
    }
}

#[cfg(test)]
#[path = "resolve_tests.rs"]
mod tests;
