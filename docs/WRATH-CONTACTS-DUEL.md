# Contacts, duels and stale core rows

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `SocialMgr.cpp`,
`SocialHandler.cpp`, `DuelHandler.cpp`, `Player.cpp` (`DuelComplete`, `SendDuelCountdown`),
`SpellEffects.cpp` (`EffectDuel`). No client-only behaviour is inferred; no Ghidra round.

The lossless `wrath_social` and `wrath_duel` codecs existed but nothing dispatched them.
`crates/benilla-protocol/src/wrath_contacts.rs` now maps them into the shared events their feeds own
(**handled**):
- 0067 contact list → shared friend list and ignore list, split by contact flags (1 friend,
  2 ignored); notes and the mute bit have no place there and are dropped. 0068 friend status →
  shared status (the result code passes through as sent; the online tail is kept).
- 0167..016B duel requested, out of/in bounds, complete, winner (mode 1 is a retreat), 02B7
  countdown (AC's milliseconds as the shared whole seconds).

Outbound through the existing encoders (route `NativeContacts`, producers in `ui_social`/`ui_duel`):
0066 contact list (flags 0x07: friends, ignores, mutes), 0069 add friend (empty note), 006A remove
friend, 006C add ignore, 006D remove ignore, 016C/016D duel accept/cancel. 006B contact notes keeps
its codec with no command or producer.

Stale rows corrected (code already native): 004E logout cancel and 004F its ack, 0036/0038/003C
character create/delete, 00A9/01F6 (compressed) update object.

006B set contact notes (`ClientCommand::SetContactNotes { guid, note }`, through the `wrath_social` encoder on the NativeContacts route) is a transport-only writer; App wires the producer.
