# Wrath guild management (build 12340)

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03` `Server/Packets/GuildPackets.cpp`
(readers and writers), `Handlers/GuildHandler.cpp`, `Handlers/NPCHandler.cpp`
(`HandleTabardVendorActivateOpcode`) and `Guilds/Guild.h` (`GuildCommandError`, `GuildEvents`,
`GuildEventLogTypes`). Codecs: `P/wrath_guild.rs` (replies), `P/wrath_guild_commands.rs`
(requests), `P/wrath_guild_rank.rs` (the rank save). The Lua getters are Native UI's
(`U/script/guild.rs`, `U/script/wrath_friends_frame.rs`); this unit feeds them.

## Replies

`wrath_guild::session_events` maps each reply onto the shared guild event where 3.3.5 means what
1.12 did, so the existing owners (`ui_guild` roster and identity, chat lines, the tabard designer,
the invite popup) consume it:

- 0055 query → `GuildQueryResponse` (the five emblem words as signed; the trailing rank count is
  dropped, the roster's rank list being the live count);
- 0083 invite → `GuildInvite`; 0088 info → `GuildInfo` (the packed creation date decoded to
  1-based day and month and the full year);
- 008A roster → `GuildRoster` (rank flags as the rights words; the raw last-online bits as the
  float), and the raw packet stays as `WrathGuild` for each rank's bank words;
- 0092 event → `GuildEvent` (3.3.5 numbering extends 1.12's; GUID tail for joined, left, signed
  on and signed off); 0093 command result → `GuildCommandResult` (3.3.5 codes extend 1.12's;
  the bank codes 25..29 have no line yet);
- 01F1 emblem save result → `SaveGuildEmblemResult`; 01F2 tabard designer → `TabardVendorActivate`;
- 03FF event log (`u8` count; per row the type, player GUID, the other GUID except for joins and
  leaves, the rank for promotes and demotes, seconds ago) stays as `WrathGuild`.

`A/ui_guild/wrath.rs` (`NativeGuild`) keeps the roster's rank bank words and the event log, and
pushes the log to `set_wrath_guild_events` with names from the name cache (an unknown GUID is
asked once and shows unnamed until answered) and rank names from our guild's identity. Both clear
on Connected and Disconnected.

## Requests

Existing producers (`ui_guild::drain_guild`, the petition and tabard ladders) now reach native
writers on the `NativeGuild` route: create 0081, leave 008D, disband 008F, leader 0090, add rank
0232, delete rank 0233, public/officer note 0234/0235, info text 02FC, emblem save 01F1 (vendor,
emblem style, emblem colour, border style, border colour, background), tabard designer 01F2 and
`QueryGuildEventLog()` (new `GuildRequest::QueryEventLog`) 03FF. `GuildControlSaveRank` sends 0231
with all six bank rows: the pane's staged edit (`wrath_guild_bank_staged_rights`) when there is
one, else the rank's words from the last roster, so saving a rank keeps its bank rights.
`CMSG_GUILD_CREATE` has no producer (guilds are founded by charter).
