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
