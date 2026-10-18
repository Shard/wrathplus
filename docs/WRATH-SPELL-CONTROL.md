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

## Capture-only consumers (follow-up unit)

One read-only Ghidra round on build 12340 `Wow.exe` traced the native handlers, registered through
`0x6b0b80`. The scripts and logs are in `/tmp/azb-wire-ghidra/caps`. The message names come from
`Wow.exe`'s message table at `0xac8af0`: 20-byte rows holding name, kind, sound, tag and chat
type, indexed by `0x5216f0`.

- **`SMSG_SPELL_FAILURE` `0x0133`** (`0x809c70`).
  - App Migration's `spell/native_failure.rs` owns it (cast-result ladder, combat log).
  - Added here: when the caster is us and the reason is `SPELL_FAILED_NEED_AMMO`, `_NEED_AMMO_POUCH`
    or `_NEED_EXOTIC_AMMO` (52-54), `0x807560(1)` stops auto-repeat. It sends
    `CMSG_CANCEL_AUTO_REPEAT_SPELL` and clears the running spell, through
    `creature_anim::cancel_auto_repeat_local`; STOP_AUTOREPEAT_SPELL follows the state edge.
- **`SMSG_PET_CAST_FAILED` `0x0138`** (`0x806c30`): the existing owner presents the seven proven
  refusals. The row is now handled.
- **`SMSG_PLAY_SPELL_IMPACT` `0x01F7`** (`0x8006c0`): the `SpellVisualKit` row and a streamed target
  make a kit play (`0x745230`). Here it is a `KitPush` on the target, as `SMSG_PLAY_SPELL_VISUAL`
  does. AzerothCore sends it from trainers (kit 362) and `.debug`.
- **`SMSG_PET_LEARNED_SPELL` / `_UNLEARNED_SPELL` `0x0499`/`0x049A`** (`0x5d4c30`): the handler formats
  the spell's name, `"%s (%s)"` when it has a rank, and prints:
  - `ERR_PET_LEARN_SPELL_S` (row `0x27f`);
  - `ERR_PET_LEARN_ABILITY_S` (row `0x280`) when Spell.dbc Attributes has `0x10`;
  - `ERR_PET_SPELL_UNLEARNED_S` (row `0x281`).

  All three rows are kind 0 (a system chat line) with no sound. They are added to the message
  catalog as `WRATH_SUPPLEMENT`. The pet book and bar themselves come from `SMSG_PET_SPELLS`.
- **`SMSG_PET_UPDATE_COMBO_POINTS` `0x0492`** (`0x5d36a0` → `0x5d34b0`).
  - AzerothCore writes `packed unit, packed target, u8 points`. The decoder read one guid and so
    refused every real packet; it is fixed.
  - The handler applies the update only when the unit is the client global at `0xc23500`. It writes
    the combo storage the player's own combo display reads, and signals the update.
  - Here the driven unit's update (`VehicleWorld::driven`) becomes `NativeComboPoints`. Reading the
    global as the controlled unit is an inference.
- **`SMSG_FEIGN_DEATH_RESISTED` `0x02B4`**: both AzerothCore senders are commented out
  (`SpellAuraEffects.cpp:2954`, `:3039`). The row moves to the out-of-scope "no AC sender" bucket,
  and its decoder stays.

## The pet's own cast and the missile reports

One read-only Ghidra round on build 12340 `Wow.exe` found the senders by their packet opener
`0x47b0a0`. The scripts and logs are in `/tmp/azb-wire-ghidra/petcast`.

- **`CMSG_PET_CAST_SPELL` `0x01F0`.** `Spell_C`'s send (`0x80ac90`) opens `0x1F0` instead of
  `CMSG_CAST_SPELL` `0x12E` when the cast's caster is not the active player, or when the spell has
  flag `0x40000`.
  - Body: the caster's full `u64` guid, then the same cast count, spell id, cast flags and
    `SpellCastTargets` (AzerothCore `HandlePetCastSpellOpcode`).
  - Here, a pet, charm or possess-bar spell whose targets are a location raises the targeting
    cursor, as ours does: `spell::cast_target::pet_cursor_word`, an implicit-target-16 spell or a
    location-only `Targets` word. It does so from the native bar press and from the pet book's
    `CastSpell(id, "pet")`.
  - The click commits `CastCommit::Pet { caster }` as `ClientCommand::NativePetCastSpell`. That
    shares one cast counter with `CMSG_CAST_SPELL` and arms none of the player's cast state or GCD.
  - Unit-targeted pet spells stay `CMSG_PET_ACTION`.
  - Inferred: that the native bar raises the cursor for these. The bar's press path was not traced.
- **Missile trajectory.** `0x80ac90` appends it to the cast itself: cast flags `0x02`, then
  elevation and speed, then the active mover's `MSG_MOVE_STOP` block. This happens when the spell's
  missile row has flag 1 and the caster is the active mover (AzerothCore `HandleClientCastFlags`).
  The elevation and speed math (`0x7fe520`) is not built.
- **`CMSG_UPDATE_MISSILE_TRAJECTORY` `0x0462` and `CMSG_UPDATE_PROJECTILE_POSITION` `0x04BE`.** No
  sender opens either through `0x47b0a0`. A byte scan finds one `push` of each, at `0x6fd744` and
  `0x70364c`, probably for an opcode-taking sender. Both stay missing and labelled.
