//! The world broadcasts: server messages, zone-under-attack and defense messages. The last two
//! print, senderless, on every joined channel whose `ChatChannels.dbc` row has `DEFENSE`
//! (0x10000); a `ZONE_DEP` (0x2) channel also needs the area's parent zone to be the player's.
