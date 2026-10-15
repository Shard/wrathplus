# Native Wrath dungeon finder plan (build 12340)

Status: **review proposal; no implementation authorized by this document**.
Scope includes dungeon finder (LFD) and the contemporary raid browser (LFR), not
modern automated raid finder. Do not turn this into a Vanilla LFG compatibility
layer. Preserve the native packet fields and independent caches; implement the
Wrath-only Lua surface and stock FrameXML events over those caches.

## Evidence and limits

- Protocol reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`,
  `src/server/game/Handlers/LFGHandler.cpp`,
  `Server/Packets/LFGPackets.{h,cpp}`, `Server/Protocol/Opcodes.{h,cpp}`,
  `DungeonFinding/LFGMgr.cpp` (including `RBPacketBuildFull`,
  `RBPacketBuildDifference`, `RBPacketAppendGroup`, `RBPacketAppendPlayer`).
  Opcode comments are not authoritative layouts: inspect the actual read/write
  bodies. In particular, most packets still live in LFGHandler, not LFGPackets.
- Installed executable, read-only: `Wow.exe`, SHA-256
  `c6f9209840a8a5941538bcda0e63c4bf445a154b6c1c9c7397be31cebabc289c`.
  Native ABI/cache inspection must precede implementing each API unit; do not
  infer Lua tuples or reset behavior from a server packet alone.
- Installed `LFDFrame.lua`, `LFGFrame.lua`, `LFRFrame.lua` and their XML:
  consumer evidence only. Local extraction is `/tmp/azb-ui-dump`; no Blizzard
  sources, executable bytes, DBC rows, or session captures belong in this repo.
- App Migration census `/tmp/azb-app-data-census.md` reports **45 missing APIs /
  80 lexical call sites**, not runtime frequency or the entire family. Existing
  `wrath_compat.lua` stubs conceal additional prerequisite APIs listed below.

## Wire inventory

Notation: counts have their stated wire widths; GUID is full `u64`, strings are
NUL-terminated, booleans retain their raw byte in decoded native state. Dungeon
entries are `u32` with type in the high byte; do not discard it during storage.
Unknown/reserved values are retained, not relabeled from guessed semantics.
All decoders are transactional, length/count checked, reject trailing bytes, and
publish nothing on failure. Use independent handwritten byte fixtures.

### Client to server

| Opcode | Message | Body / source contract |
|---|---|---|
| `035c` | CMSG_LFG_JOIN | `u32 roles`, no-partial-clear byte, achievements byte, `u8` slot count + `u32` entries, needs-count byte + three needs bytes, comment. LFGPackets::LFGJoin::Read; native sender must confirm validation/count rules. |
| `035d` | CMSG_LFG_LEAVE | Empty. |
| `0296` | CMSG_LFG_GET_STATUS | Empty; server answers both player and party status, one with cleared dungeon selection. |
| `0362` | CMSG_LFG_PROPOSAL_RESULT | `u32 proposalId`, accept byte. ID must come from current proposal, not dungeon ID. |
| `0366` | CMSG_SET_LFG_COMMENT | Comment string. |
| `036a` | CMSG_LFG_SET_ROLES | `u8` roles (different width from JOIN/role snapshots). |
| `036c` | CMSG_LFG_SET_BOOT_VOTE | Agree byte. |
| `036e` | CMSG_LFD_PLAYER_LOCK_INFO_REQUEST | Empty. |
| `0371` | CMSG_LFD_PARTY_LOCK_INFO_REQUEST | Empty. |
| `0370` | CMSG_LFG_TELEPORT | Out byte. |
| `035e` / `035f` | CMSG_SEARCH_LFG_JOIN / LEAVE | `u32` entry; AC masks type on join, reads the value on leave. Retain native sender convention. |
| `036b` | CMSG_LFG_SET_NEEDS | Named opcode but AC Handle_NULL / STATUS_NEVER. Do not invent support or use instead of JOIN's needs fields. |

### Server to client

| Opcode | Message | Lossless snapshot / transition |
|---|---|---|
| `0367` | SMSG_LFG_UPDATE_PLAYER | Update-type byte, extra-info byte; when present: queued byte, two reserved bytes, `u8` entries + comment. |
| `0368` | SMSG_LFG_UPDATE_PARTY | Separate state, not alias of PLAYER: update/extra, join/queued bytes, **five** reserved bytes, `u8` entries + comment. |
| `02bb` | SMSG_LFG_ROLE_CHOSEN | GUID, ready byte, `u32` roles. |
| `0363` | SMSG_LFG_ROLE_CHECK_UPDATE | `u32` state, initializing byte, `u8` entries, `u8` rows `(GUID,ready,u32 roles,u8 level)`. Preserve order: AC sends leader first. |
| `0364` | SMSG_LFG_JOIN_RESULT | `u32 result`, `u32 state/value`, optional party lock block. AC emits block only when nonempty; prove result-dependent native framing before accepting arbitrary optional suffixes. |
| `0365` | SMSG_LFG_QUEUE_STATUS | `u32 dungeon`, five **signed i32** waits (average, own, tank, healer, damage), three needs bytes, `u32 queuedTime`. Preserve negative unavailable-wait sentinels. |
| `0361` | SMSG_LFG_PROPOSAL_UPDATE | Entry, state byte, proposal ID, encounter mask, silent byte, `u8` rows `(u32 roles,self,inDungeon,sameGroup,answered,accepted bytes)`. Rows do **not** carry GUIDs; do not fabricate identities. |
| `036d` | SMSG_LFG_BOOT_PROPOSAL_UPDATE | In-progress/did-vote/agree bytes, victim GUID, `u32` votes/agrees/time-left/needed, reason. |
| `036f` | SMSG_LFG_PLAYER_INFO | `u8` random reward rows: entry, done byte, money/XP/two reserved `u32`s, `u8` item triples `(item,display,count)`; then player lock block. |
| `0372` | SMSG_LFG_PARTY_INFO | Party lock block: `u8` members, each GUID + player lock block. Player lock block is `u32` count + `(entry,u32 reason)` rows. |
| `01ff` | SMSG_LFG_PLAYER_REWARD | Random entry, actual entry, done byte, extra `u32` (AC writes 1), money/XP/two reserved words, `u8` item triples. Separate completion cache from random-dungeon preview rewards. |
| `0293` | SMSG_LFG_OFFER_CONTINUE | `u32` dungeon entry; backfill offer is not automatic queue acceptance. |
| `0200` | SMSG_LFG_TELEPORT_DENIED | `u32` reason (despite AC helper taking `u8`). |
| `0398` | SMSG_LFG_DISABLED | Empty. |
| `0369` | SMSG_LFG_UPDATE_SEARCH | One update byte. Not the browser result list. |
| `0360` | SMSG_UPDATE_LFG_LIST | Type/dungeon, delta flag, optional deleted-GUID list, group and player sections with counts/reserved words and flag-gated fields. AC LFGMgr builders are essential, not just LFGHandler. Full replaces; delta merges/deletes by identity while preserving absent fields. |
| `0515` | SMSG_OPEN_LFG_DUNGEON_FINDER | Named in Opcodes; no writer in the audited LFGHandler. Close payload and event contract with native handler before implementing. |

Browser codec must retain native member/group GUIDs, comments, role bits, area,
status, bind GUID/encounter mask, and all stat fields, including float item-level
and ratings. AC deliberately repurposes one stat field as online status; preserve
raw wire data rather than baking that server-specific convention into generic UI.
Unknown flag bits require a proven framing rule, not silently ignoring bytes.

## Installed executable findings (static, read-only)

All addresses are preferred-image VAs (base `0x00400000`), not file offsets or
live-process addresses. Raw PE disassembly found the 67-entry registration table
at `0x00ACD668`, walked by registrar `0x005531D0`. Bounded branch-aligned local
logs are `/tmp/lfg-native-audit/`; these are evidence aids, not runtime inputs.
No executable, project, or server was modified and no game process was launched.

| API / native entry | Observed backing state and constraints |
|---|---|
| GetLFGQueueStats `0x00553F90` | Dungeon `0x00BEAA28`, wait words `0x00BEAA2C..3C`, needed-role bytes `0x00BEAA40..42`, timestamp-like value `0x00BEAA48`. Packet writer branch `0x0055CD3E` is identified by LFG_QUEUE_STATUS debug string. Getter also gates on player/group objects `0x00ACD9A0`/`0x00ACDAC0`; valid path returns 13 values, initial failure zero. Exact additional-wait-word/timestamp conversion still needs closure. |
| GetLFGProposalMember `0x00553110` → `0x00553000` | Active header `0x00BEA980`; five records at `0x00BEA994`, stride `0x18`. 1-based indices 1..5; seven returns. Role bits select TANK/HEALER/DAMAGER, final name/class returns are nil: this getter does not resolve names. Writer branch `0x0055C76C` has LFG_PROPOSAL_UPDATE string. Do not fill anonymous rows with guessed roster names. |
| GetLFGRoleUpdateSlot `0x00554740` → `0x00554690` | Packed dungeon vector count/pointer `0x00BEAA54/58`, four-byte entries; returns high-byte type and low-24-bit ID. Separate role-member vector count/pointer `0x00BEAA64/68`, stride `0x18`. Writer `0x0055C966/980` identifies LFG_ROLE_CHECK_UPDATE. Slots are not member records. |
| GetLFDLockInfo `0x00557930` | Usage `(dungeonID,playerIndex)`; index 1 self record `0x00ACDBE0`, valid byte `0x00ACDBE8`; party validity/head `0x00ACDC08/14`. Each player has GUID and separate linked lock list; reason zero when absent/unlocked. Validity writers `0x0055D0BF/0x0055D112`; same lock state feeds joinability helper `0x00559C90`. |
| SearchLFGGetResults `0x00555FD0` | Index mapping helper `0x00553530` uses separate presentation vectors (count/pointer `0x00BEAB8C/90` and `0x00BEAB9C/A0`); `0x00ACD660` changes concatenation order with group sort. RefreshLFGList `0x0055D280` rebuilds presentation object `0x00BEAB20` from backing object `0x00BEAA98` through `0x0055B1A0`, cloning records. Backing and sorted presentation are not one array. |
| SetLFGDungeon `0x00559F50` → `0x00559B70` | Metadata-validated packed key, local selection object `0x00ACD880`, head/count `0x00ACD894/898`, dirty byte `0x00ACD883`; helper deduplicates and may clear incompatible choices. No direct send in traced setter/helper/insert path; full flush/send lifecycle remains open. |

Shared metadata lookup uses globals `0x00AD404C/50/60` (maximum/minimum/row
pointers); identifying its exact DBC binding remains a separate loader audit.
GUID/name helper `0x0067D770` is used by locks/browser, not proposal-member names.
Static addresses establish cache separation, not every field meaning or reset
rule. Complete getters/writers/command senders for the remaining APIs, event
payloads and invalidation are explicit per-unit prerequisites below.

## Native state boundaries and lifecycle

The implementation schema must distinguish:

1. **Local choices:** selected dungeon IDs/entries, chosen roles, comment,
   enabled/header-collapse preferences, browser joined ID and sort state.
   A local selection is not a server-confirmed queue membership.
2. **Catalog:** localized immutable dungeon/group metadata and ordering, including
   texture, description, difficulty/type, faction, expansion, flags and bounds.
3. **Player and party queue status:** separate update streams plus latest signed
   wait statistics and observation time. Do not collapse an absent extra block
   into an explicit empty selection without native transition evidence.
4. **Role check:** native state, ordered participants, selected slots, response and
   role masks, levels. Keep GUID lookup separate from row ordering.
5. **Proposal:** current ID, entry, state, encounter mask, silent flag and ordered
   anonymous rows. API-derived name/class/leader fields need native provenance.
6. **Locks/reward previews:** player locks, per-member party locks, random dungeon
   reward rows. Reason codes, empty reply, never requested and stale cache differ.
7. **Completion, vote and backfill:** separate completion reward, boot-vote state,
   continuation offer and teleport error, not mutations of the ordinary roster.
8. **Raid browser:** full/delta group and player records, removed GUIDs, joined
   query and native ordering/sort/filter presentation; no synthetic queue rows.

Packets update app state before their UI events. Multiple transitions in one tick
must replay in order, not disappear behind a latest-value comparison. Prefer
sequence-numbered bounded/acknowledged notifications plus current snapshots over
another session-unbounded journal; reset epochs must prevent cursor aliasing.
Socket teardown clears session data; character change, group leave, zone change,
ReloadUI and request/reply invalidation each need explicit native evidence and
regressions. ReloadUI must not resend queue/proposal decisions. No optimistic
server facts from Join/Leave/accept/vote/teleport commands.

## Catalog work (Assets)

`LFGDungeons.dbc` AC DBCStructure/DBCfmt describes **49 fields**: ID; 16 localized
name offsets + mask; min/max/target/target-min/target-max levels; map, difficulty,
flags, type, signed faction; texture offset; expansion; order; group; 16 localized
description offsets + mask. AC skips faction/texture/order/descriptions in its
server struct: the client cannot copy that lossy subset. Keep ID and wire entry
separate (`id | type << 24`).

Assets verified installed availability through the MPQ chain: `LFGDungeons`
is **195 rows × 49 fields, 196-byte records, 3692 string bytes**;
`LFGDungeonGroup` is **10 rows × 21 fields, 84-byte records, 237 string bytes**.
Both are WDBC headers from the effective Wrath enUS chain. No existing formats readers/mapped layouts were
found. Header availability is verified; installed column semantics and API
behavior are not thereby verified. The 49-field mapping above is AC source
coverage, to cross-check against the client loader before implementing.

`LFGDungeonGroup.dbc` supplies client headers/group ordering. Assets must verify
its installed loader mapping and localized fields rather than infer layout from
LFGDungeons.GroupID. Both parsers need strict header/record/string bounds,
locale fallback, signedness checks, and synthetic fixtures. Loader/catalog belongs
in benilla-assets; byte layouts in benilla-formats. Check installed availability
through the MPQ chain; no extracted tables committed.

Reward values/locks are server data, not DBC guesses. Item names/links/icons use
existing item query/display stores. Encounter names/order need a separate audit
of installed encounter/map metadata and native getters; an encounter bitmask
alone cannot produce names. Deserter/random-cooldown APIs require their native
unit/aura/timer source, not arbitrary constant durations or only self data.

## API units and owners

The following covers all **45** census names; companion APIs below are required
for the already-stubbed family to become useful. Each unit first closes Lua
argument coercion, invalid-index returns, nil/false distinctions, tuple arity and
event shapes against native registration/getters and installed consumers.

| Unit | Census APIs | Proposed implementation split |
|---|---|---|
| L0 Catalog/local selection | ClearAllLFGDungeons, GetLFDChoiceCollapseState, GetLFDChoiceEnabledState, GetLFDChoiceInfo, SetLFGDungeon, SetLFGDungeonEnabled, SetLFGHeaderCollapsed, SetLFGRoles, SetLFGComment | Assets catalog; UI choices/ABI; App forwards only proven outbound intents. |
| L1 Locks/random previews | GetLFDChoiceLockedState, GetLFDLockInfo, GetLFDLockPlayerCount, GetLFGRandomDungeonInfo, GetRandomDungeonBestChoice, GetLFGDungeonRewardInfo, GetLFGDungeonRewardLink, RequestLFDPartyLockInfo, RequestLFDPlayerLockInfo | Descriptor PLAYER/PARTY_INFO codecs; App cache/request lifecycle; UI tuple/reward/link presentation. |
| L2 Queue/role check | JoinLFG, LeaveLFG, CompleteLFGRoleCheck, GetLFGRoleUpdateSlot, GetLFGQueueStats | Descriptor JOIN/status/role codecs; App independent snapshots/ordered transitions; UI commands/readers/events. |
| L3 Proposal | AcceptProposal, RejectProposal, GetLFGProposalMember, GetLFGProposalEncounter | Descriptor proposal codecs; App active-ID and ordered response state; UI tuple/encounter mapping after native provenance closure. |
| L4 In-dungeon/completion/vote | GetLFGBootProposal, SetLFGBootVote, GetLFGCompletionReward, GetLFGCompletionRewardItem, GetPartyLFGBackfillInfo, PartyLFGStartBackfill, IsInLFGDungeon, LFGTeleport, UnitHasLFGDeserter, UnitHasLFGRandomCooldown | Descriptor completion/vote/continue/teleport codecs; App instance/aura/cache integration; UI native decisions/timers. |
| L5 Raid browser | SearchLFGGetEncounterResults, SearchLFGGetJoinedID, SearchLFGGetNumResults, SearchLFGGetPartyResults, SearchLFGGetResults, SearchLFGJoin, SearchLFGLeave, SearchLFGSort | Descriptor full/delta codec; App keyed lossless store; UI native result tuples/order/sort. Separate milestone, not omitted as a modern-LFR mismatch. |

Companion stub inventory: GetLFGDungeonInfo, GetLFDChoiceOrder,
GetNumRandomDungeons, GetLFGRoles, GetLFGMode, GetLFGQueuedList, GetLFGInfoServer,
GetLFGRoleUpdate, GetLFGProposal, GetLFGDungeonRewards, HasLFGRestrictions,
IsPartyLFG, IsLFGDungeonJoinable, IsListedInLFR, CanPartyLFGBackfill,
GetLFGDeserterExpiration, GetLFGRandomCooldownExpiration, RefreshLFGList. Audit
all installed calls before defining the final surface; removal of one shim must
not expose a missing companion API.

### File/ownership contracts

- **Descriptor:** `benilla-protocol/wrath_lfg/` strict native types/decoders and
  command bodies; dedicated native SessionEvent path, no Vanilla projection.
  Wire owns NetCommand enum/transport routing and encoding integration; Descriptor
  supplies independently tested body codecs. Coordinate schemas before edits.
- **App Migration:** dedicated `benilla-app/ui_lfg/` (not growth in ui_party),
  session-scoped native reducer/store and lifecycle tests; feed into UI-owned
  frozen DTO/API using semantic player reads, never inferring queue/group state
  from Vanilla party; item/aura/instance joins; ordered request
  drain through Wire-owned commands; synthetic replay and installed census.
  App Migration confirmed this proposed split and has no additional verified
  LFG evidence beyond its census. It does not add duplicate transport owners.
- **Native Wire:** confirmed opcode registration, dispatch/event transport,
  outbound command encoding and routing, using Descriptor's strict codec/types.
  App Migration is the sole reducer/capture owner per event; Wire adds no parallel
  reducer. Native metadata crosses an explicit lossless event seam rather than
  being inferred from shared projection. Confirmation is for plan review only.
- **Native UI:** `benilla-ui/script/wrath_lfg/`, native Model snapshot/choices,
  Wrath-only registration/exports and ABI/event tests. Retire matching compat
  stubs only as real bindings land; Vanilla surface unchanged. Native UI confirmed
  the binding/status-event/intent-queue/ABI/stock-integration scope. Its existing
  shims are inventory only, not native ABI evidence; the six executable paths
  above were audited separately for this plan.
- **Assets:** confirmed formats owns typed locale-aware parsers/catalogs;
  assets exposes/caches through the coordinated override layer. No queue state,
  eligibility, networking or Lua policy in Assets. All mod.rs/model.rs edits additive
  and coordinated. Workers confirmed these boundaries for review; manager
  approval remains required. Do not start implementation from this plan.

## Delivery and gates

1. Review this plan and close native evidence gaps; agree exact packet/state/feed
   contracts and API units. Approve L0/L1 first, not a 45-function stub sweep.
2. Assets catalog and Descriptor independent L0/L1 codec fixtures can proceed in
   parallel **after approval**; UI native ABI work can be read-only meanwhile.
3. Ship L0+L1 end-to-end with actual empty/nonempty replies, localized catalog,
   requests and stock choice UI. Then L2 queue/role check; then L3 proposal; then
   L4 in-dungeon lifecycle; finally L5 browser as its own substantial codec unit.
4. Each behavior unit runs full affected protocol/UI/formats/assets suites and
   app `--lib`; independent wire fixtures include truncation at every boundary,
   trailing data, counts, zero/missing, high-byte types, signed waits, raw flags,
   malformed transactional rejection, full/delta deletion, and back-to-back
   start/finish/reset. Commands assert exact opcodes and byte bodies.
5. Native Lua tests cover tuple lengths/coercion/invalid inputs, party/self and
   anonymous proposal identities, preview vs completion rewards and companion
   stubs removed. Events are Wrath-shaped, not gated against 1.12 signatures.
6. Installed headless census: stock LFD/LFR traversal, select/clear/query, simulated
   lock/queue/role/proposal/boot/completion/browser replies, zero hard failures;
   report residual family gaps rather than declaring success from panel load.
   Manager owns live play/real session capture; workers never launch Wow.exe or
   alter the server. Sanitize any future replay fixture before commit.

Builds use each worker's own target, `CARGO_INCREMENTAL=0`, four jobs, `nice -n 15`,
shared heavy lock, clean unit-test environment and required test thread limits.
No runtime cutover flag, no fabricated queues/locks/rewards, no server changes.
