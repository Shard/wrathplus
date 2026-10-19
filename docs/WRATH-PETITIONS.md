# Wrath guild and arena-team charters (build 12340)

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03` `Handlers/PetitionsHandler.cpp`.
Codec: `P/wrath_petition.rs`. The existing shared owner (`A/ui_petition`) consumes the replies and
produces the commands; this unit makes them native.

Against 1.12:
- `SMSG_PETITION_SHOWLIST` rows add a required-signature count after the flags word (dropped: the
  shared row has no field for it);
- `SMSG_PETITION_QUERY_RESPONSE` has seven words between the body text and the gender (1.12 had a
  flags word first; it reads 0 now) and ends with the charter type, 0 guild or 1 arena team
  (dropped);
- `CMSG_PETITION_BUY` is the NPC, a word, a GUID, the name, an empty string, seven words, a short,
  three words, ten empty strings, the charter index (1: the registrar sells one guild charter; AC
  reads it for arena-team charters only) and a word;
- `CMSG_OFFER_PETITION` leads with a word AC skips.

Unchanged and decoded natively: show signatures, sign results, decline, turn-in results, rename;
the show-list, show-signatures, sign, turn-in, query, rename and decline requests.

## Registrars

AC answers `SMSG_PETITION_SHOWLIST` with one guild row from a tabard designer, else the three
arena rows (2v2/3v3/5v5, charters 23560-23562). The row word 1.12 read as a visibility flag is the
charter type: 0 for guild, 2/3/5 for arena. So the guild registrar no longer reads it; before, the
1.12 `entry_flags & 1` check silently dropped every 3.3.5 guild showlist. `ui_petition::net::show_list`
routes by the NPC's flags, labelled because the client's dispatch is untraced:
- both petitioner and tabard designer: the guild registrar, as before;
- a petitioner alone: the arena petition vendor (`ui_petition/vendor.rs`, an NPC session with the
  walk-away guard, and the `"npc"` unit).

The vendor's Lua (`U/script/wrath_petition_vendor.rs`) follows Experience's Wow.exe pass
(`/tmp/azb-exp-misc-clean.log`):
- `GetPetitionItemInfo(index)` (`0x5a1fb0`) → name, texture, price; nil, nil, 0 for a row out of
  range or whose item is not cached yet.
- `BuyPetition(index, name)` (`0x5a2530`) runs the shared name check, then sends 01BD with the row
  index.
- `ClosePetitionVendor()` (`0x5a24f0`) fires `PETITION_VENDOR_CLOSED` (`0x1e2`) itself.
- `HasFilledPetition()` (`0x5a2820`).
- `TurnInArenaPetition(teamSize, bg rgb, emblem, emblem rgb, border, border rgb)` (`0x5a26d0`)
  queues the bracket and a five-word banner: three opaque ARGB colours and the two style ids, all
  `0xffffffff` when an argument is missing. App sends it after the charter's GUID on 01C4, as AC
  reads it (`PetitionsHandler.cpp:790`).

App publishes the rows (`set_wrath_petition_vendor`) and the bag scan
(`set_wrath_has_filled_petition`). It fires `PETITION_VENDOR_SHOW` on the open,
`PETITION_VENDOR_UPDATE` when a row's item lands or the scan flips, and `PETITION_VENDOR_CLOSED`
on a walk-away.

Labelled, because the reference's `0x6dd680` and the event fire sites are untraced:
- "filled" means an arena charter is carried;
- the turn-in takes the bracket's charter, and sends nothing without one;
- the colour packer (`0x61be50`) is taken as the inverse of the tabard reader's `ARGB / 255`.
