# Trade, mail, auction and loot remainders

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `TradeHandler.cpp`
(`SendUpdateTrade`), `SharedDefines.h` (`TradeStatus`), `MailHandler.cpp`, `Player::SendMailResult`,
`AuctionHouseHandler.cpp`, `AuctionHouseSearcher.cpp` (`BuildAuctionInfo`), `Group.cpp`
(`MasterLoot`), `LootHandler.cpp`, `GroupHandler.cpp`. No client-only behaviour is inferred; no
Ghidra round. Adapter: `crates/benilla-protocol/src/wrath_exchange.rs`.

These replies mean what their 1.12 counterparts mean, so they translate at the wire into the shared
events the existing feeds own (**handled**): 0120/0121 trade status and window (the existing
lossless `wrath_trade` codec; trade codes 0..=22 match 1.12, 23 `NOT_ON_TAPLIST` stays `Unknown(23)`;
the window requires AC's seven rows; gem enchant words are dropped), 0239 send-mail result
(`wrath_mail_mutations` framing; the item-taken tail carries the item's low GUID where 1.12 carried
the entry, unused by the feed), 0255 auction hello (the enabled byte is framed and dropped),
025B command result (AC's extra word on any successful non-sell action is a bid's min-outbid; on a
cancel it is framed and dropped), 025C/025D/0265 list pages (148-byte rows: the permanent enchant of
seven triples is kept, the search delay dropped), 025E/025F notices (the owner notice's trailing
float dropped), 02A4 master-loot candidates.

**Capture-only**: 0297 show-mailbox (existing `NativeMailMutationPackets`, which no longer receives
send-mail results), 0490 pending sales (`NativeExchangePackets`; AC always sends zero rows). Both
clear on Connected and Disconnected.

Outbound (all routes need only the live writer; no server acceptance is claimed):
- Trade 0116..011F: the existing `ClientCommand`s through `wrath_trade::Command`; accept sends 1.
- Auction 0255/0256/0257/0258/0259/025A/0264: the existing commands. Sell sends one `(GUID, count)`
  pair; `AuctionSellItem` gains the stack count, read from the item at send time. List-items sends
  `getAll` 0 and no sort block.
- 02A3 master-loot give: the existing command.
- Mail 0238/0245..024A: `ui_mail` produces `NativeMailMutation` from the native inbox in build
  12340 (row id, sender, template, attachment low GUID); the legacy mail variants stay unrouted.
  The Lua `TakeInboxItem` binding records no attachment index, so a take names the row's first
  remaining attachment. An open marks an unread row read; letter text is inline, so no item-text
  query is sent. Send-mail sends package 0 and AC's trailing constants as 0.
- Transport-only `ClientCommand::NativeExchange`: 048F pending-sales query, 0409 opt-out of loot.

Not actionable: 0164 loot item notify and 04FD loot slot changed (registry only in AC), 028D
auction removed notification (AC never sends it), 0109 store loot in slot (AC `Handle_NULL`).
