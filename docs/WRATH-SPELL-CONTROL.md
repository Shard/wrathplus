# Spell control, pet and visual feedback

Wire reference: AzerothCore (`../azerothcore-playerbots/core/src/server/game`):
`Player::_StartCooldown` and `AddSpellMod` (Player.cpp), the login spell-mod resend
(CharacterHandler.cpp), `Pet::RemoveSpellCooldown` (Pet.cpp), `Spell::SendChannelStart`,
`SendChannelUpdate`, `SendResurrectRequest` (Spell.cpp), `Unit::SendTameFailure`,
`SendPlaySpellVisual`, `SendPlaySpellImpact`, `SendPetActionFeedback`, `SendPetActionSound`,
`SendPetDismissSound`, `SendComboPoints` (Unit.cpp), `WorldSession::SendPetNameInvalid`
(PetHandler.cpp), `TotemCreated` (TotemPackets.cpp), and for requests the `WorldSession::Handle*`
readers in PetHandler.cpp, SpellHandler.cpp and MiscHandler.cpp plus PetPackets.cpp/TotemPackets.cpp.
No client-only behaviour is inferred; no Ghidra round. Adapter:
`crates/benilla-protocol/src/wrath_spell_control.rs`.

Strict framing throughout: a short body or a trailing byte is `Unhandled`, never a fallback.

**Handled** (shared events, existing owners):
- 0135 cooldown event, 01DE clear cooldown: `u32 spell, u64 guid` → the self or pet cooldown store.
- 0139/013A channel start/update: build 12340 names the caster (packed) and sends it to the whole
  set. Every caster reaches the shared `ChannelStart`/`ChannelUpdate` with `caster: Some(guid)`; the
  self channel bar acts on ours only (for App Migration's unit cast events, which own other bars).
- 015B resurrect request: `u64 caster, u32 nameLength`, the name with AC's terminator, AC's second
  terminator, `u8 sickness`, then an optional `u32 0` delay override (`SPELL_ATTR3_NO_RES_TIMER`),
  read as `has_timer`.
- 0173 tame failure, 02C6 pet action feedback, 0324/0325 pet sounds: the 1.12 shapes. AC's tame
  reasons 12 (exotic) and 13 (unknown) fall to the existing table's default key; the 3.3.5
  client's own table is not proven here.
- 0178 pet name invalid: `u32 PetNameInvalidReason`, the name, and the declined forms when flagged,
  all carried on `PetNameInvalid`; the line stays the generic `ERR_INVALID_PETNAME`.
- 01F3 play spell visual → the existing kit playback.
- 039D combo points: build 12340 carries combo points in no update field, so the descriptor
  accessor returned `None` and every finisher was greyed and refused locally. The packet
  (`packed target, u8 points`) now feeds `NativeComboPoints` (`net/wrath_combo_points.rs`), read
  through `banked()` by `GetComboPoints`/`PLAYER_COMBO_POINTS`, the usable gate and the cast ladder.
  Cleared on Connected and Disconnected.

**Spell modifiers**: 0266/0267 decode losslessly (`u8 bit, u8 op, i32 total`) into `spell/mods.rs`,
whose tables now hold all 96 family bits (`SpellDisplay::has_family_bit`, from Assets' third
family word, ba707306). The tooltip dependency mask widened with them (`u128`).

**Capture-only** (`NativeSpellControlPackets`, `net/wrath_spell_control_capture.rs`, ordered with
duplicates, cleared on Connected and Disconnected): 01F7 spell
impact, 0413 totem created.

**Outbound** (`WrathRoute::NativeSpellControl`, admission fixtures in `wrath_admission_tests.rs`),
bodies as AC's handlers read them; no server acceptance is claimed:
- With existing producers: 013B cancel channelling, 015C resurrect response (status 1 accept,
  0 decline), 0174 pet set action (one or two pairs; AC counts them by body size), 0175 pet action,
  0176 abandon, 0177 rename (no declined names), 026B pet cancel aura, 026D cancel auto-repeat,
  02EA pet stop attack, 02F3 pet spell autocast.
- Transport-only (`ClientCommand::NativeSpellControl`, no producer yet): 0279 request pet info,
  029B cancel growth aura, 0375 cancel mount aura, 03F8 spell click, 0414 totem destroyed,
  047A pet learn talent.

**Not in this unit**: 01F0 pet cast spell (needs a native `SpellCastTargets` writer and a
producer), the vehicle seat/exit requests 046D/0476..0479 (they carry movement info; movement
family), 0179 pet spells (already native; its row is unchanged).

**Not actionable** (registry only, no AC send site; undecoded, `REGISTRY_ONLY`): 014D resume cast
bar, 017A pet mode, 0252 resurrect failed, 02AF pet broken, 02F1 pet unlearn confirm, 0330 chain
targets, 03A3..03A6 obsolete extra aura info, 0403 force display update, 044C play dance,
0475 pet renameable.
