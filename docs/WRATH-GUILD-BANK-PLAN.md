# Native Wrath guild bank plan (build 12340)

Status: **review proposal; no implementation authorized by this document**.
Keep guild-bank state native and separate from personal bank, inventory and
Vanilla guild records. This plan covers the missing core-UI surface and the
load-on-demand guild-bank prerequisites; the census is not a complete API list.

## Evidence and limits

- AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`:
  `Server/Packets/GuildPackets.cpp:164–205,239–438`, corresponding header types,
  `Handlers/GuildHandler.cpp:265–407`, `Guilds/Guild.cpp:1869–1936`, and
  `Server/Protocol/Opcodes.h`. Inspect actual reads/writes, not packet allocation
  sizes: money-withdrawn allocates eight bytes but writes **one signed i32**.
- Original executable SHA-256
  `c6f9209840a8a5941538bcda0e63c4bf445a154b6c1c9c7397be31cebabc289c`.
  Wire's first bounded audit confirms the bank-list framing and query senders
  listed below. Permissions and rank-edit storage/save now have native evidence below;
  remaining lifecycle and addon audit is in progress.
  Server layouts alone are not proof of Lua ABI or native lifecycle. Do not
  implement an unresolved tuple, event or state transition.
- Native evidence artifacts: `/tmp/azb-wire-guildbank-{refs,table,readers,senders,
  state-refs,handler,extra}.log` (local only). Registration `0x005a77d0` maps
  `03e8` to `0x005a7250`, `03ee` to `0x005a4800`, `03fe` to `0x005a4ab0`,
  `040a` to `0x005a4ae0`. Native reader helpers `0x0047b340`, `0x0047b3c0`,
  `0x0047b400` consume one, four and eight bytes respectively.
- Native bank-list handler `0x005a7250` confirms the `03e8` conditional framing:
  u64 balance; tab; four-byte remaining value; full byte; metadata only when
  full is nonzero and tab zero; counted slot records; item-zero short records;
  conditional random-property seed; counted socket rows. Retain raw fields;
  native signed sentinel interpretation still requires getter/policy evidence.
- Native senders `0x005a5800`, `0x005a58c0`, `0x005a5940`, `0x005a5c40`
  emit respectively tab query (`03e7`: active-bank GUID, tab, cache flag), log
  query (`03ee`: tab), text query (`040a`: tab), and text set (`040b`: tab,
  C string). Cache flag semantics and guard/reset policy need further tracing.
- App Migration census `/tmp/azb-app-data-census.md:14–15`, source
  `/tmp/azb-app-census.log`, base `8e51087b`: **16 APIs / 23 lexical call sites**
  in core FrameXML plus TimeManager traversal, not all load-on-demand addons or
  runtime frequency. Installed consumer sources stay outside the repository.
- `FriendsFrame.lua:1647–1695,1738–1746,1920–1950` shows permission editing,
  pending/cancel UI state and subsequent `GuildControlSaveRank`. MerchantFrame
  uses repair eligibility. These are consumer clues, not native implementation.
- Verified baseline `f9fcf0fb`: App/UI search finds only the
  `Blizzard_GuildBankUI` addon-list entry, no guild-bank owner/feed/actions.
  Existing protocol seams: `wrath_vendor` repair guild-bank byte and
  `NpcService::GuildBanker` presence bit. The latter does not authorize actions:
  AC bank handlers interact with a **guild-bank gameobject**, not that NPC flag.

## Additional native permissions, allowance and cost evidence

- Registration `0x005cd0b0` maps `03fd` to `0x005cb9f0`: reads four-byte
  rank, flags and money, one-byte purchased count, then exactly six four-byte
  rights/remaining pairs irrespective of that count. Preserve raw words; the
  AC signed declarations do not alone prove every getter interpretation.
  Evidence: `/tmp/azb-wire-guildbank-permission-handler.log`.
- `GetGuildBankWithdrawMoney` assembly at `0x005a44a3` uses `FILD dword ptr
  [0x00c1dc34]`: signed conversion and negative sentinel preservation are
  confirmed, not inferred from the packet reader's unsigned helper.
- `GetGuildBankTabCost` (`0x005a6b20`) requires a player and purchased count
  below six, then indexes `0x00a15eec + 4*purchased`. Nonzero cost returns a
  number; otherwise nil. Six native values, in copper: **1,000,000; 2,500,000;
  5,000,000; 10,000,000; 25,000,000; 50,000,000**. These are native constants,
  not server pricing policy or authority to predict purchase success.
- `CanGuildBankRepair` (`0x005cc200`) checks own guild exists and own rank
  rights mask **0x40000**, returning **1/nil**. Do **not** add balance,
  remaining-allowance, open-bank or merchant-context gates to this Lua predicate.
  Repair execution and the merchant UI may have their own independent checks.
  Signed getter, cost constants and repair evidence:
  `/tmp/azb-wire-guildbank-cost-asm.log`.

## Native mutation boundaries

Evidence: `/tmp/azb-wire-guildbank-{mutations,swap,senders}.log`.
`0x005a5010` sends buy-tab `03ea` with active GUID and purchased-count tab;
`0x005a5b00` / `0x005a5ba0` send deposit/withdraw with active GUID and u32
amount, only with nonzero active GUID. Lua deposit `0x005a5e20` converts the
numeric argument to a four-byte value and checks it against player money,
reporting error `0x28` when insufficient; withdrawal `0x005a5ef0` converts and
passes to its sender without adding a local balance/allowance predicate.
Do not introduce symmetric validation by assuming the two wrappers match.

Money pickup `0x005a4330` (in `rank.log`) converts its numeric amount to a
u32, requires nonzero and either nonzero balance high-word or amount no greater
than the balance low-word, then calls cursor helper `0x005208f0` with amount,
kind `0x0c` and event ID `0x021d`. It emits no withdrawal packet in this wrapper;
do not interpret money pickup as server transfer or add a rank-allowance gate.

Item pickup `0x005a6200` and split `0x005a6600` operate through the existing
cursor machinery, using cursor kind `0x0b` for bank items and one-based Lua
indices converted to zero-based native tab/slot (six tabs, 98 slots). Pickup
branches between inventory-origin, bank-origin and empty-cursor behavior;
split validates a positive amount against item count and sets cursor state.
Neither should be reduced to an unconditional server swap. Auto-store
`0x005a63f0` uses an active bank and emits `03e9` with bank-only=0,
auto-store=1, retained item/count, to-slot=1 and stack-count=0.
Tab update wrapper `0x005a6a00` resolves a one-based icon index before calling
`0x005a5a00`; its sender validates/normalizes the name and sends `03eb` with
active GUID, tab, name and resolved icon string. Keep cursor transitions,
item-cache lookups, icon resolution and validation in their respective owners,
not in the lossless packet encoder. G4 fixtures must cover each sender branch
in `0x005a50c0` / `0x005a5410`, not just AC reader acceptance.

## Missing census surface

Counts are lexical sites. Exact arguments, Lua result types, sentinels and event
ordering require native audit before each implementation unit.

| Function | Sites | Proposed dependency / question to prove |
|---|---:|---|
| BuyGuildBankTab | 1 | Active banker + next tab; native cost/index/guard policy. |
| CanGuildBankRepair | 2 | Native own-guild existence and own-rank rights mask `0x40000`; returns 1/nil, without balance or interaction gates. |
| CloseGuildBankFrame | 1 | Local interaction close; cache retention and event behavior. |
| DepositGuildBankMoney | 2 | Active banker + amount; sender width/validation. |
| GetGuildBankMoney | 2 | Replicated bank balance, not player copper. |
| GetGuildBankTabCost | 1 | Native next-tab table indexed by purchased count; requires player and count below six, nonzero value or nil. |
| GetGuildBankTabInfo | 1 | Tab metadata and rights; full tuple/unavailable semantics. |
| GetGuildBankTabPermissions | 1 | Selected rank-edit cache, not necessarily current-member permissions. |
| GetGuildBankWithdrawLimit | 1 | Selected rank daily limit; gold/copper units and unlimited sentinel. |
| GetGuildBankWithdrawMoney | 1 | Signed current-member remaining withdrawal, preserving negative sentinel; distinct from rank limit. |
| GetNumGuildBankTabs | 2 | Purchased tab count, distinct from populated/known tab caches. |
| PickupGuildBankMoney | 1 | Cursor-money local state; not an immediate server withdrawal assumption. |
| SetGuildBankTabPermissions | 1 | Shared selected-rank rights mutation; UI pending/cancel is separate. |
| SetGuildBankTabWithdraw | 1 | Shared selected-rank item allowance, not a standalone opcode. |
| SetGuildBankWithdrawLimit | 2 | Shared selected-rank money word; asymmetric getter/setter/save conversion below. |
| WithdrawGuildBankMoney | 3 | Active banker + amount; no optimistic balance mutation. |

## Installed load-on-demand consumer inventory

Read-only extraction evidence: `/tmp/azb-ui-guildbank-consumers/consumer-summary.txt`,
`native-sites.txt`, `global-inventory.txt`. TOC/Lua resolve from patch-enUS-2,
XML from patch-enUS, localization from locale-enUS, using explicit patch precedence.
No installed sources belong in the repository.

The addon calls **27 bank globals**; eight overlap the core census, producing
**35 combined globals**, plus **`GameTooltip:SetGuildBankItem(tab,slot)`**.
The 19 additional globals are AutoStoreGuildBankItem, CanEditGuildTabInfo,
CanWithdrawGuildBankMoney, GetCurrentGuildBankTab, GetGuildBankItemInfo,
GetGuildBankItemLink, GetGuildBankMoneyTransaction, GetGuildBankText,
GetGuildBankTransaction, GetNumGuildBankMoneyTransactions,
GetNumGuildBankTransactions, PickupGuildBankItem, QueryGuildBankLog,
QueryGuildBankTab, QueryGuildBankText, SetCurrentGuildBankTab,
SetGuildBankTabInfo, SetGuildBankText and SplitGuildBankItem.
`IsTabViewable` is Lua-defined, not a missing native binding. Tabard/icon APIs,
cursor APIs, money-frame/popups and guild-control rank selection/save remain
cross-family prerequisites rather than fabricated bank-local substitutes.

Consumer tuples (not proof of full native arity or invalid-input semantics):
- Tab info: name, icon, isViewable, canDeposit, numWithdrawals,
  remainingWithdrawals (Lua:345,436).
- Item info: texture, itemCount, locked (Lua:232; XML:16).
- Item transaction: type, name, itemLink, count, tab1, tab2, year, month, day,
  hour (Lua:573). Money transaction: type, name, amount, year, month, day,
  hour (Lua:605). Preserve elapsed wire values; date/name/link presentation
  requires its native reader path, not a fabricated receipt timestamp.
- Negative money remaining and negative item withdrawals display unlimited;
  positive means count and zero none (Lua:466+,657), corroborating signed getter.

Eleven registered addon events (Lua:86–96): GUILDBANKBAGSLOTS_CHANGED,
GUILDBANK_ITEM_LOCK_CHANGED, GUILDBANK_UPDATE_TABS, GUILDBANK_UPDATE_MONEY,
GUILDBANK_UPDATE_WITHDRAWMONEY, GUILD_ROSTER_UPDATE, GUILDBANKLOG_UPDATE,
GUILDTABARD_UPDATE, GUILDBANK_UPDATE_TEXT, GUILDBANK_TEXT_CHANGED, PLAYER_MONEY.
The addon ignores events while hidden. UPDATE_TEXT forwards tab arguments;
TEXT_CHANGED compares the first argument to current tab and requests text.
UIParent separately handles GUILDBANKFRAME_OPENED/CLOSED, loads the addon,
shows/hides it and can call CloseGuildBankFrame if not visible. Panel OnHide
also calls close; close must therefore tolerate this consumer recursion.
GuildBankInfo OnHide invokes Save (XML:1059–1061). These consumer dependencies
are not proof of packet receipt order or native reset/cancel behavior.
The money and guildbankmoney cursor types have different XML drop handling;
retain the existing cursor owner and distinguish those kinds explicitly.

## Wire inventory (AzerothCore reference; native corroboration noted below)

Notation: GUID = full u64; strings = C strings. Decode boolean fields as raw u8.
Preserve signed/raw values, ordered rows and explicit empty slot records. All
bodies are transactional, count/length checked before allocating and reject
trailing data. Publish no event or partial cache change on malformed input.

### Client to server

| Opcode | Message | Body |
|---|---|---|
| `03e6` | GUILD_BANKER_ACTIVATE | GUID banker, full-update byte. |
| `03e7` | GUILD_BANK_QUERY_TAB | GUID banker, u8 tab, full-update byte. |
| `03e9` | GUILD_BANK_SWAP_ITEMS | GUID banker, bank-only byte, conditional branches below. |
| `03ea` | GUILD_BANK_BUY_TAB | GUID banker, u8 tab. |
| `03eb` | GUILD_BANK_UPDATE_TAB | GUID banker, u8 tab, name string, icon string. AC validates lengths separately; native limits require proof. |
| `03ec` / `03ed` | DEPOSIT / WITHDRAW_MONEY | GUID banker, **u32** amount despite u64 bank balance. |
| `03ee` | GUILD_BANK_LOG_QUERY | u8 tab, with special money-log tab convention to prove. |
| `03fd` | GUILD_PERMISSIONS | Empty. |
| `03fe` | GUILD_BANK_MONEY_WITHDRAWN | Empty request; distinct reply below. |
| `040a` | QUERY_GUILD_BANK_TEXT | u8 tab. |
| `040b` | SET_GUILD_BANK_TEXT | u8 tab, text string. |
| `0231` | GUILD_RANK | u32 rank, u32 flags, rank-name string, u32 daily money limit, **six** `(u32 tabFlags,u32 itemLimit)` rows. Rank edits share guild control; do not emit individual setter packets. |

Swap branches (`GuildBankSwapItems::Read` and header widths):
- Bank-only: destination `(u8 tab,u8 slot,u32 item)`, source same triple,
  auto-store byte, i32 bank-item-count.
- Not bank-only: `(u8 bankTab,u8 bankSlot,u32 item)`, auto-store byte; when
  auto-store: i32 bank-item-count, u8 to-slot, i32 stack-count; otherwise
  u8 container, u8 container-slot, u8 to-slot, i32 stack-count.
- Preserve native direction/sentinel conventions only after sender tracing.
  Do not conflate these with Vanilla inventory swap packets.

### Server to client

| Opcode | Message | Lossless body |
|---|---|---|
| `03e8` | GUILD_BANK_LIST | u64 money, u8 tab, i32 withdrawals-remaining, raw full-update byte; if tab==0 and full-update, u8 metadata count + ordered `(name,icon)` strings; u8 item-record count + conditional slot records below. |
| `03ee` | GUILD_BANK_LOG_QUERY | u8 tab, u8 count, ordered event records below. |
| `03fd` | GUILD_PERMISSIONS | u32 rank, i32 guild flags, i32 money limit, u8 purchased-tab count, **six** `(i32 flags,i32 remainingItems)` rows, not purchased-count rows. Native handler `0x005cb9f0` confirms fixed six rows. |
| `03fe` | GUILD_BANK_MONEY_WITHDRAWN | **i32** remaining money, including negative sentinel. |
| `040a` | QUERY_GUILD_BANK_TEXT | u8 tab, text string. |

Slot record: u8 slot, u32 item ID; when ID nonzero, i32 flags, i32 random
property ID, optional i32 seed when property ID nonzero, i32 count, i32 permanent
enchantment, u8 charges, u8 socket-count + `(u8 index,i32 enchant)` rows.
Item ID zero is an explicit empty slot, not a missing record. Do not lose
negative random-property IDs, charge bytes, enchantments or ordering. Native
item-link assembly must use the retained fields plus existing item-cache data.

Log record: raw u8 kind, GUID actor, conditional payload, then u32 elapsed.
**Native handler `0x005a4800` differs from AC's default branch:** native money
payload is one u32 only for kinds **4/5/6/8/9**; every other kind reads u32 item
and u32 count, adding u8 destination-tab for **3/7**. Therefore a native codec
must use this native predicate rather than AC's default-money branch. Preserve
unknown kinds and their native-framed payload without inventing meanings or
converting elapsed offsets to fabricated absolute times. Evidence:
`/tmp/azb-wire-guildbank-log-handler.log`. Native storage uses 25 log rows per
tab (stride `0x28`); this is not evidence for imposing a 25-row wire-count cap.

Additional required seams: `SMSG_GUILD_ROSTER` (`GuildPackets.cpp:59–73,194–205`)
contains rank flags/money limits and six tab-right/limit pairs per rank, followed
by members. `SMSG_GUILD_COMMAND_RESULT`, guild membership/rank updates, bank
activation through gameobject use, and inventory/cursor errors must route to
their existing sole owners or acquire one explicit owner, not duplicate reducers.

## Native state and ownership proposal

Wire's bounded audit locates six native tabs: metadata at `0x00c11458` with
stride `0x2144` (icon +`0x40`, text +`0x140`); per-tab slot arrays at
`0x00c1dc40`, counts at `0x00c1dc3c`, per-tab stride `0x10`, 98 slots of
`0x2c` bytes each. Bank balance is retained at `0x00c1dc20/24`; separate
four-byte values at `0x00c1dc30` and `0x00c1dc34` must not be aliased.
Active bank GUID is `0x00c1dc10/14`, pending GUID `0x00c1dc18/1c`.
Reset `0x005a7760` clears allocations/tab count/current tab and marks query
flags dirty. Direct callers are the own-player guild-ID field watcher
`0x006e1bb0` (registered by `0x006e45d0`, type 4, offset `0x0c`, width 4)
and bank-subsystem teardown `0x005a7830`. No direct map-reset caller was found;
this does not prove every cross-map lifecycle path. These addresses
are evidence, not a design requirement to duplicate the native memory layout.
The dedicated bank binding table contains **29 functions**, a larger scope
than the 16-name core census, and excludes the cross-family rank-edit bindings.

These are **separate stores to verify**, not asserted complete lifecycle policy:

1. Session/guild identity and current banker interaction. Scope replies by the
   actual protocol/native rules; packets without banker GUID cannot be given a
   fabricated correlation token. Prove close, map transfer, guild leave/change,
   disconnect and reconnect behavior; never leak another session's bank cache.
   Native close `0x005a4500` -> `0x005a4270` clears active/pending banker GUIDs
   and dispatches event ID `0x0219`, with **no wire packet or item-cache clear**.
   Event `0x0219` maps to `GUILDBANKFRAME_CLOSED`. Guild event handler
   `0x006d92d0` case 8 closes and clears tab metadata; flag watcher
   `0x0071a0b0` closes a matching active banker on loss of `0x800000`.
   Neither close path implies the full cache reset. Evidence:
   `/tmp/azb-wire-guildbank-lifecycle{,-registration,-final}.log` and
   `/tmp/azb-wire-guildbank-event-{init,asm,table,refs}.log`.
2. Purchased tab metadata and per-tab slot caches, with known/unavailable
   separate from zero/empty. Native `03e8` clears all 98 slot fields through
   `0x005a4b60` only when full is nonzero **and packet tab is zero**, using the
   handler's selected-tab target, before applying listed rows. Initially
   unallocated tab storage is also zeroed. Other updates retain unmentioned
   slots: **full with nonzero tab does not itself clear slots**. Confirm the
   selected-tab target in reducer fixtures rather than broadening to every tab.
3. Bank u64 balance, member remaining money and per-tab remaining withdrawals;
   current-member permission reply remains distinct from roster rank definitions.
4. Selected rank and shared retained rank arrays, including all six tab rows.
   **Do not invent an isolated native draft:** `SetRank` (`0x005ca260` ->
   `0x005ca090`) selects a zero-based rank and copies 17 flag-checkbox states;
   `SetRankFlag` (`0x005ca2b0`) edits that checkbox array. Bank tab permission
   setter `0x005cb470` edits `0x00c21e68[rank*14+tab]`; tab withdrawal setter
   `0x005cb760` edits the parallel `0x00c21e80` storage directly. Money-limit
   setter `0x005cb6a0` edits `0x00c21e64`, capped at 1e9. Getter `0x005cb700`
   divides a nonnegative signed word by 10000 but preserves a negative word.
   Save `0x005cb170` sends `0231` with rank, composed flags, name,
   **stored money word multiplied by 10000**, then six ordered rights/slot
   pairs. Preserve this evidenced asymmetric storage/conversion behavior;
   do not normalize it into a hypothetical clean draft or claim server success
   from local setter mutation. Installed GuildControl Cancel only hides the
   popup; OnHide/OnShow call no native restore/reset or pending-table clear.
   Tab edits are Lua-buffered; Accept invokes money setter, pending tab setters,
   rank save, Hide, then clears Lua pending tables. Those tables also clear on
   initial load and changed rank; same-rank cancel/reopen can retain them.
   Rank-checkbox setters remain immediate and separate. No native rollback
   follows from Cancel. Consumer evidence:
   `/tmp/azb-ui-guildbank-consumers/guild-control-cancel.txt`.
   Evidence: `/tmp/azb-wire-guildbank-{rank-table,rank,cache-rank}.log`.
5. Per-tab text/log snapshots and local selected tab, cursor item/money state;
   do not infer a transfer from cursor pickup or clear cursor on unrelated reply.

Descriptor owns lossless schemas/codecs and independent fixtures. Wire owns
opcode registration, generic command routing and adapter tests. App owns the
sole reducer, lifecycle, capture, ordered command drain and UI feed. UI owns
Wrath-only Lua ABI/events and consumer integration. Assets owns static/item/icon
lookups. Manager owns integrated gates and live checks. No server changes or
client launch by workers; no proprietary data in commits.

## Stages and acceptance gates

- **G0 — proof and prerequisites:** native handlers/senders/bindings and addon
  inventory; document exact Lua tuples, signed sentinels, indices/units, events,
  close/reset policy, full/delta semantics, costs and guild-control rank seams.
  Resolve ownership with App/UI/Wire; request approval before implementation.
- **G1 — balance/permissions/metadata:** bounded strict readers, necessary roster
  rank extension, query/activate commands, sole App state and read-only Wrath
  getters. Cover full versus delta, zero versus unavailable, six permission rows
  versus purchased count and malformed no-publish behavior. No fake defaults.
- **G2 — money/cost/repair and close:** verified amount conversion and validation,
  purchase/deposit/withdraw, local cursor-money behavior, exact repair-rights
  predicate and separately owned repair execution/allowance checks, plus
  explicit interaction lifecycle. Generic repair sender remains singly owned;
  test failure/no optimistic balance edits and separate bank/merchant contexts.
- **G3 — rank edits:** selected-rank reads, direct shared-rank mutations,
  checkbox state, UI pending/cancel behavior and shared GUILD_RANK sender.
  Preserve unedited rights and the evidenced asymmetric money conversions;
  do not isolate native setters into an invented draft or alias current-member
  allowance with selected-rank limits. Test permission loss, guild/rank changes,
  save/cancel, repeated reads/edits and invalid inputs.
- **G4 — complete addon path:** native-proven slot operations, tab name/icon/text,
  logs, links and events. Do not claim guild-bank completion from the 16-name
  core census alone. Scope can be split into smaller reviewed commits.

Each unit runs its touched crate's full `--lib` suite and locked App `--lib`
with pasted counts; Wrath-only binding changes also run UI full suite and
Vanilla reference-surface checks. Use own target, jobs=4, incremental=0,
`nice -n 15`, test threads=4; unset profile/locale and empty WOW_DATA for unit
suites. Installed headless census uses Wrath enUS environment and is a separate
serialized gate. Keep own target under 40 GB. Independent fixtures cover every
conditional branch, truncation at every boundary, count overflow/trailing data,
raw sentinel preservation and native limits versus codec framing. Integration
replays exercise adapter -> single FIFO -> sole reducer -> UI, event order,
reset/reconnect and rejected-packet nonmutation. No live client by workers.

## Review decision and stage-local proof gates

Manager approved committing this documentation unit and beginning **G1 message
readers**. Remaining uncertainties do not block the bounded lossless codecs;
no approval is implied for unproven lifecycle, Lua policy or later-stage behavior.

- **G1 integration gate:** direct reset callers and close event name are now
  proven above. Preserve the distinction between reset, metadata clear and
  close. Cover session/guild transitions in reducer fixtures; do not invent a
  standalone map reset. Verify notification ordering before Lua emission.
  Money pickup event `0x021d` is `GUILDBANK_UPDATE_MONEY`, not `CURSOR_UPDATE`.
- **G2 gate:** distinguish the proven rights-only repair predicate from repair
  execution contexts, allowance checks and cursor delivery. Do not invent extra
  predicate gates or a successful repair from local eligibility.
- **G3 gate:** installed Cancel behavior is now traced above: no native rollback.
  Direct native setters mutate shared arrays; Lua pending edits do not prove
  array restoration. Add fixtures for Accept ordering and pending-table lifetime.
  App/UI must agree a sole guild-control/roster rank owner before that integration.
- **G4 gate:** complete branch-specific sender and getter ABI fixtures before
  slot/log/text behavior lands. Consumer destructuring is not full ABI proof.

First implementation unit is strict read-only bank message schemas/decoders,
not App reducers, routing, queries, Lua bindings or guild-control edits. Each
later stage remains separately reviewable and must close its relevant proof
questions before behavior implementation.
