# Item/inventory family

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`.
`Server/Packets/{ItemPackets,BankPackets}.{h,cpp}`, `Handlers/{ItemHandler,SpellHandler,
BankHandler,QueryHandler,CharacterHandler}.cpp`, `Entities/Item/Item.cpp`, `Entities/Player/
{Player,PlayerStorage}.cpp` establish every layout. No client-only behavior is inferred; no
Ghidra round required. Codec: `crates/benilla-protocol/src/wrath_inventory.rs`.

Receive opcodes decoded into existing shared events (**handled**): 005B page text,
00B0 item cooldown, 01B8 show bank, 01BA buy-bank-slot result, 01EA item time,
01EB item enchant time (its trailing owner GUID is framed, not kept). AC also sends
`ERR_BANKSLOT_OK` (3); the bank feed has no message for it and shows nothing.

Receive opcodes that are **capture-only** (`NativeInventoryPackets` FIFO, sole owner, cleared on
Connected and Disconnected, duplicates kept): 00AE/00AF read item ok/failed, 01D7 enchantment log,
02C5 item-set name. Reasons: 01D7 carries a SpellItemEnchantment id where the shared combat-log event wants a spell id;
0244 is keyed by item GUID while the letter cache is keyed by text id. Raw codes, zeros and
ignored-slot values (raw GUID 1) are preserved. Strict framing rejects truncation and trailing
bytes; malformed packets dispatch as unhandled.

Consumers take their receipts out of that FIFO and leave the rest in order:

- **Refunds** (`A/net/wrath_item_refunds.rs`): every refundable item carried or worn
  (`ITEM_FIELD_FLAG_REFUNDABLE`) is asked once per session, eagerly on sight (04B3; when native asks is untraced); 04B2 becomes the
  ContainerFrame's `GetContainerItemPurchaseInfo`/`Item` row for its slot (money, honor, arena,
  cost items with icon and nine-field link), counted down from the reply's played time over AC's
  two-hour window and dropped once expired; a successful 04B5 removes the row, a refused one keeps
  it.
- **Equipment sets** (`A/net/wrath_equipment_sets.rs`, Lua in `U/script/wrath_equipment.rs`):
  AC sends 04BC only at login, so the client keeps its own copy, by server index. The gear
  manager's `SaveEquipmentSet` (worn items, ignored-for-save slots as raw 1; an existing name keeps
  its index and GUID, a new one takes the first free index of ten) sends 04BD and updates the copy
  at once; 0137 brings a new set's GUID. `DeleteEquipmentSet` sends 013E (a set the server has not
  numbered yet is deleted once 0137 brings its GUID). `UseEquipmentSet` sends 04D5 with each slot's item and
  its current bag/slot; 04D6 fires `EQUIPMENT_SWAP_FINISHED(result == 0, name)`. A change to the
  copy fires `EQUIPMENT_SETS_CHANGED`. Labelled: an item id the client has no object for (banked)
  reads 0 in `GetEquipmentSetItemIDs`; `EquipmentSetContainsLockedItems` is always false;
  `PickupEquipmentSetByName` holds nothing (no action-bar set buttons); the set tooltip shows the
  name only.
- **Letters**: a mail-made letter's text is asked by the item's GUID in 12340 (0243, from the
  item-text reader, `A/ui_item_text.rs`), and 0244's answer goes to the reader's `LetterTexts`
  cache instead of the FIFO. Labelled: AC's "no text" answer carries no GUID, so it answers
  nothing and the reader waits.
- **Sockets** (`A/net/wrath_sockets.rs`, Lua in `U/script/wrath_sockets.rs`, gem tables in
  `benilla-formats` `gems.rs`): `SocketContainerItem`/`SocketInventoryItem` open a session on a
  carried item. Its sockets are the template's socket blocks (now kept by the item-query codec as
  `SocketMetadata`, delivered as `WrathItemSocketMetadata`), plus a colourless one while enchant
  slot 6 is set. Existing gems are enchant slots 2..4, named back to the gem item by
  `SpellItemEnchantment` column 33; a gem's colour is its template's `GemProperties` row. A gem
  fits a socket on a shared colour bit; a prismatic socket takes any but a meta gem.
  `ClickSocketButton` takes the cursor item as the socket's new gem (one gem fills one socket),
  or takes the new gem back with an empty cursor; `AcceptSockets` sends 0347 with the new gems;
  050B (AC `Item::SendUpdateSockets`, sent on success only) clears them. The window opens on
  `SOCKET_INFO_UPDATE`, closes on `SOCKET_INFO_CLOSE` when the item leaves the bags, and
  `CloseSocketInfo` ends the session quietly. Tooltips (`SetSocketedItem`, `SetSocketGem`,
  `SetExistingSocketGem`) show the nine-field links. Labelled: a refused socketing gets no reply
  from AC, so its new gems stay placed; a gem's own bag slot is not locked while placed.

Outbound: existing `ClientCommand`s now have native writers (route `NativeInventory`):
open, wrap, auto-equip, set ammo, swap-inv (destination first in this build), swap, auto-store
bag, split (u32 count), destroy (three trailing zero bytes), banker activate, buy bank slot,
autobank, autostore bank, page text query. `UseItem` gains the spell id and item GUID that
build 12340's `CMSG_USE_ITEM` carries (AC refuses a GUID mismatch); it rides the session route
to share the cast counter with `CMSG_CAST_SPELL`, glyph index and cast flags 0, target block with
a u32 mask. `ClientCommand::NativeInventory` carries the inventory-only commands: refund info and equipment
set save/delete/use have the producers above; read item, item-set name query, item text query by
GUID, auto-equip slot, refund and cancel temp enchantment are transport-only; socket gems has
the socketing producer.
Routes need the live writer only; they claim no server acceptance.

Not in this unit: 0057/0059 item query multiple (AC `Handle_NULL`/registry only), 00B2 destroy
items and 0106/0107 ground items (AC `Handle_NULL`), the Vanilla-shaped `ItemTextQuery` (text
id; mail family), 0164 loot item notify (loot family), guild bank swaps.
