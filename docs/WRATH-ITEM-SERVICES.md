# ContainerFrame refund/socket boundary

Build-12340 client-only evidence: `GetContainerItemPurchaseInfo` 0x5d8d80,
`GetContainerItemPurchaseItem` 0x5d8f70, `SocketContainerItem` 0x5d8b10,
`SocketInventoryItem` 0x5ea6a0, socket getters 0x5c49d0..0x5c5470.
Audit logs: `/tmp/azb-ui-container-family-{refs,native}.log`.

Refund rows are host snapshots, not inferred from merchant prices. No row or an
expired row returns zero values. A row returns five signed numbers; cost item
queries return texture/nil, signed count, link/nil. Equipped rows use bag key 0.
The host must refresh remaining seconds and remove ineligible/expired rows.
`set_item_refunds` replaces the entire view without emitting receipt events.

Socket queries currently implement the native **closed/unavailable session**:
item info is nil,nil,0; socket count is 0; gem info is three nils; links and socket
types return zero values; refundable is nil. There is no fabricated item/gem state.

## Explicit capture-only operations (not live functional claims)

`take_item_service_requests` drains ordered SocketContainer, SocketInventory and
WithdrawGuildBankMoney requests. App has not wired this new drain yet. It must
validate native item availability, unlocked status/socket metadata before opening
socket UI, and guild-bank session/permissions before sending a withdrawal. Socket
open/update/commit events are not synthesized. Accept/Click/Close while the socket
session is closed send nothing. Refund packet lifecycle and a populated socket
session remain host work; adding names to the census is not evidence they work.

The census lexer masks comments and string literals before discovering calls.
Tests retain genuine Lua/XML handler calls while excluding prose and string data.
Guarded optional addon callbacks are not fabricated as engine APIs.
