# Spell and aura combat logs

Wire reference: AzerothCore (`../azerothcore-playerbots/core/src/server/game`): `Unit::SendHealSpellLog`,
`SendEnergizeSpellLog`, `SendPeriodicAuraLog`, `SendSpellNonMeleeDamageLog`, `SendSpellDamageResist`,
`SendSpellDamageImmune`, the damage-shield block of `DealMeleeDamage` (Unit.cpp),
`WorldObject::SendSpellMiss` (Object.cpp), `Spell::SendLogExecute`, `InitEffectExecuteData` and the
`ExecuteLogEffect*` writers (Spell.cpp), `EffectInstaKill`, `EffectDispel`, `EffectStealBeneficialBuff`
(SpellEffects.cpp), and the two script callers (`spell_mage_burnout_trigger`,
`spell_item_draenic_pale_ale`). No client-only behaviour is inferred; no Ghidra round.
Adapter: `crates/benilla-protocol/src/wrath_logs.rs`.

Every packet is framed strictly: a short body, an over-long count or a trailing byte is
`Unhandled`, never a fallback decode. The packets translate into the shared events the combat log
(`crates/benilla-app/src/combat_log`) already owns (**handled**):

- 0150 heal: `overheal` and `absorb` now ride `SpellHealLog` beside `critical`.
- 0151 energize: decoded natively (packed GUIDs), no longer through the 1.12 parser.
- 024B miss: moved off the 1.12 parser with its unchecked tail; raw GUIDs, the debug floats framed
  when the flag is set (AC always writes 0).
- 024C execute: packed caster, `u32 spell, u32 effectCount`, then per effect `u32 effect,
  u32 rowCount` and rows. All row GUIDs are packed, so the 1.12 parser (raw `u64` rows, 1.12 effect
  ids) is not reused. Row shape by effect, from the handler that calls each writer: 8 POWER_DRAIN
  and 62 POWER_BURN (packed target, amount, power, multiplier); 19 extra attacks; 24 create item
  (entry); 68 interrupt (target, spell); 101 feed pet (`ExecuteLogEffectDestroyItem`, entry);
  111 durability damage (target, item, slot); a bare packed GUID for 18, 28, 33, 50, 56, 76, 83,
  102, 104..107 and 113. Any other effect is rejected: AC never writes one, and its width is not on
  the wire.
- 024E periodic: damage ticks keep `overkill` and `critical`, heal ticks `overheal`, `absorb` and
  `critical`. The school arrives as a mask and was passed through raw, so a shadow tick named
  school 32; it is now the mask's lowest bit, the `SPELL_SCHOOL<n>_NAME` index the line reads.
- 024F damage shield: raw GUIDs; `spell_id` and `overkill` carried, school mask named by index.
- 0250 spell damage: moved from `wrath_session`; `overkill` carried, the trailing debug byte framed
  (AC writes the hit-info debug bits there but never the floats they announce).
- 0260 proc resist, 0263 immune: one raw-GUID body.
- 0262 dispel failed: raw caster and victim, the dispel spell id (new in build 12340, now carried),
  then whole spell-id words to the end of the body.
- 027B dispel: the dispel spell id, the header byte and each aura's flag (AC: 0 dispelled, non-zero
  cleansed) now ride `SpellDispelLog` (`auras: Vec<DispelledAura>`).
- 032F instakill: caster then victim, raw; the caster is carried.

The new fields are retained on the events; no consumer presents them yet (the chat families are the
1.12 sentences, which have no overheal or overkill slot).

**Capture-only**: 0333 spell steal, the dispel layout with AC's per-aura "0 steals, non-zero
transfers" flag, into `NativeSpellLogPackets` (`net/wrath_spell_log_capture.rs`), ordered with
duplicates, cleared on Connected and Disconnected. The client's chat families have no stolen-aura
sentence and none is invented.

**Not actionable** (registry only, no AC send site; left undecoded, `REGISTRY_ONLY`): 014F spell
break log, 01D1 aura cast log, 01D6 resist log, 027C damage calc log, 03AA spell chance proc log,
0404 spell chance resist pushback.
