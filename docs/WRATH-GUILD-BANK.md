# Wrath guild bank (build 12340)

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03` `Server/Packets/GuildPackets.cpp`
(`GuildBankQueryResults`, `GuildPermissionsQueryResults`, `GuildBankRemainingWithdrawMoney`,
`GuildBankLogQueryResults`, `GuildBankTextQueryResult`, `GuildBankSwapItems`) and
`Handlers/GuildHandler.cpp`. Codecs (existing): `P/wrath_guild_bank*.rs`. The Lua getters are
Native UI's (`U/script/wrath_friends_frame.rs`, `U/script/wrath_guild_bank.rs`); this unit is the
wire and the App feed (`A/ui_guild_bank.rs`), as planned in docs/WRATH-GUILD-BANK-PLAN.md.

Opening: a right-click on a guild vault (`GAMEOBJECT_TYPE_GUILD_BANK`, 34) in reach records the vault
(`GuildBankOpen`) and the feed sends `CMSG_GUILD_BANKER_ACTIVATE` (full update); the list's answer
opens the frame. `CloseGuildBankFrame` forgets the vault and the contents; so do Connected and
Disconnected.

Replies (now dispatched): 03E8 list (money, today's item withdrawals left, tab names and icons on a
full tab-0 update, slot contents — a full update replaces the tab, a partial one patches the named
slots and item 0 empties one), 03FD permissions (per-tab rights and item withdrawals), 03FE money
withdrawable today (-1 unlimited), 03EE logs (tab 6 the money log), 040A tab text. They become
Native UI's `WrathGuildBankView` (slots with icons and nine-field links, logs with names from the
name cache, rank rights from the guild roster's bank words), and this feed fires the frame's events
from the view's change: `GUILDBANKFRAME_OPENED`/`CLOSED`, `GUILDBANK_UPDATE_TABS`,
`GUILDBANK_UPDATE_MONEY`, `GUILDBANK_UPDATE_WITHDRAWMONEY`, `GUILDBANKBAGSLOTS_CHANGED`,
`GUILDBANK_UPDATE_TEXT(tab)` and `GUILDBANKLOG_UPDATE`.

Requests (Lua 1-based, wire 0-based): `QueryGuildBankTab` → 03E7, `QueryGuildBankText` → 040A,
`QueryGuildBankLog` → 03EE (tab 7 → the money log), `SetGuildBankText` → 040B,
`SetGuildBankTabInfo` → 03EB, `AutoStoreGuildBankItem` → 03E9 auto-store with the slot's count,
`PickupGuildBankItem` with a bag item on the cursor → 03E9 inventory-to-bank (the cursor clears),
`WithdrawGuildBankMoney` → 03ED; buy tab and deposit money are App Migration's popups on the
`NativeGuildBank` route, naming `GuildBankOpen::banker()`.

A held bank item (Native UI's mode-11 cursor, picked up or split client-side) becomes
`MoveItem` → 03E9 bank-to-bank (destination first; count 0 the stack), `DropOnBag` → 03E9
bank-to-inventory into the named bag slot, `DropOnEquip` → the same into the worn slot
(container 255).

Labelled: picking up money is not wired; slots are never shown locked; the frame does not close
by distance.

## Tooltips, tab icons and the log scroll

- `GameTooltip:SetGuildBankItem(tab, slot)` (`0x6304a0`) shows the slot's item through its link.
  The link carries the suffix, enchant and gems that the reference reads from the slot. It errors
  "Invalid tab or slot in SetGuildBankItem" without two numbers.
- `GetNumMacroItemIcons`/`GetMacroItemIconInfo` (`0x5664e0`/`0x564f60`) list the tab-icon
  chooser's item icons. `GetMacroItemIconInfo` answers `Interface\Icons\<name>`, or "" out of
  range. App builds the list once per VM: every `ItemDisplayInfo.dbc` inventory icon, each name
  once, by display id. Labelled: the reference's builder `0x565840` is untraced.
- `ScrollingMessageFrame:SetScrollOffset(offset)` (`0x973470`) sets the log's scroll, counted in
  lines up from the newest. Labelled: its clamp is untraced, so it keeps the other scroll entries'
  range.
