# Wrath LFG L1 lifecycle audit

Status: native evidence for contract review; App implementation and UI DTO are not yet frozen.
Scope: player/party locks and random reward **previews**, not completion rewards (L4).

## Method and reproducibility

Read-only static analysis of installed build-12340 `Wow.exe`, SHA-256
`c6f9209840a8a5941538bcda0e63c4bf445a154b6c1c9c7397be31cebabc289c`.
Addresses below are preferred-image virtual addresses (base `0x00400000`).
Ghidra 12.0.4 headless decompilation used an App-owned copy of the analyzed project,
`-readOnly -noanalysis`; neither executable nor other workers' projects were modified.
No game process was launched. Bounded instruction checks used Xcode `llvm-objdump`.
Local evidence: `/tmp/azb-app-lfg-audit/lifecycle{1..16}.log`, `requests.asm`,
`invalidation.asm`, `clock.asm`, `clock-constructor.asm`; prior lock evidence `/tmp/lfg-native-audit/lock-writers.asm`.
These files are audit aids, not runtime inputs. No executable bytes or Blizzard
source are committed here.

## Explicit requests: send, throttle, and cache effects

Lua registration pairs at `0x00ACD838/3C` identify
`RequestLFDPlayerLockInfo` → `0x005569D0`; `0x00ACD840/44` identify
`RequestLFDPartyLockInfo` → `0x00556A50`.

Each wrapper has its own counter and last-clock value:

| Query | Counter | Last clock | Empty-body sender | Opcode |
|---|---|---|---|---|
| Player | `0x00ACDD60` | `0x00BEABB8` | `0x00555990` | `0x036E` |
| Party | `0x00ACDD64` | `0x00BEABBC` | `0x00555A00` | `0x0371` |

The wrapper increments its counter **before** testing it. Values ≤2 send.
For values >2 it calls clock `0x0086D400`: elapsed <10.0 rejects; otherwise
it stores the current clock, resets the counter to zero, and sends. The 10.0
single-precision constant is at `0x009E30CC`. This is not a sliding-window
"two requests per ten seconds" algorithm: the acceptance that resets the counter
is itself followed by two counter-only acceptances. Rejected calls still increment.
Player instructions `0x005569D3..0x00556A3A` independently confirm the decompilation;
party is the corresponding `0x00556A53..0x00556ABA` path.

### Initialization, lifetime, and clock origin

The PE `.data` words at `0x00ACDD60/64` initialize both counters to **2**.
The last-clock fields at `0x00BEABB8/BC` lie in the zero-initialized virtual tail
of `.data`. Thus the first explicit call increments to 3 and tests the clock;
there are **not** two unconditional free calls at process startup. Two counter-only
acceptances follow a threshold acceptance that resets the counter to zero.
All literal code references to these four globals are in the two request wrappers;
the audited cache/session/UI teardown paths do not reset them. Retain the same
shared synchronous gate across VM replacement and session/cache resets.

Clock `0x0086D400` calls `0x0086AE20`, interprets its integer result as unsigned
32-bit (negative signed results add `4294967296.0`, constant `0x009E23AC`), then
multiplies by the widened single-precision constant `0.0010000000474974513`
at `0x009E1134`. Request timestamps are stored back as `f32`; preserve that
quantization rather than changing boundary behavior with an all-`f64` clock.

`0x0086AE20` loads singleton `0x00D4159C` and jumps to `0x0086ADC0`.
The latter reads `QueryPerformanceCounter` for clock type 2, otherwise
`GetTickCount`, multiplies by the object's double scale, adds its double offset,
and converts to integer. The FPU operations are confirmed by `clock.asm`, since
the decompiler omits that arithmetic. Constructor `0x0086AEA0` sets scale to
`1000.0 / frequency` (fallback frequency 1000), and offset to **zero**.
`0x0086AB30` probes frequency/reliability and chooses the clock type; it does not
subtract a process-start origin. `0x0086AE70` only returns the frequency.
Singleton creation/destruction `0x0086AF20/60` allocates/frees this clock.
Consequently the input is OS-counter-derived milliseconds, **not** elapsed time
since VM, session, or process creation. The shared gate needs an injectable
OS-monotonic-millisecond source with native unsigned-32 wrapping and seconds
conversion; replacing it with VM-relative `GetTime` changes initial and boundary
behavior. Native attach refresh remains outside this explicit-request gate.

Accepted calls push Lua boolean true via `0x0084E4D0`; rejected calls push false.
Both return one value. Neither wrapper nor sender clears validity, empties rows,
sets a native pending flag, or waits for a reply. Senders construct only the opcode
and call transport `0x006B0B50`. Their calls are independent of cache validity.
**Do not invalidate a valid reply merely because another request was accepted.**
The executable has no request ID in these packets: do not invent reply matching,
request-dependent acceptance, timeouts, or an authoritative pending/stale state.
Host instrumentation may track sent requests separately from native cache validity.

## Arrival: replacement before notifications

Shared packet dispatcher: `0x0055BDC0`.

### PLAYER_INFO (`0x036F`)

Branch `0x0055CF7E` clears self validity (`0x00ACDBE8`), clears the old
self lock list, and destroys old preview rows through `0x00559800`.
It reads all preview rows/items, then calls `0x00559B10` for the self lock block.
At `0x0055D0BF` it sets self validity to 1, including for an empty reply.
It calls `0x0055AAF0` to rebuild the derived random-dungeon choice list, then:

1. `0x0055D0CD` → event `0x201`, `LFG_LOCK_INFO_RECEIVED`, no arguments.
2. `0x0055D0D9` → event `0x202`, `LFG_UPDATE_RANDOM_INFO`, no arguments.

Party locks are not replaced by this packet.

### PARTY_INFO (`0x0372`)

Branch `0x0055D0F2` clears the previous party store using `0x0055AE20`,
reads ordered party lock records through `0x0055B080`, and sets party validity
(`0x00ACDC08`) to 1 at `0x0055D112`, even for zero members.
After `0x0055AAF0` it sends the same event pair, in the same order:
`0x0055D120` (`0x201`), then `0x0055D12C` (`0x202`), both no arguments.
Self locks and preview rewards are not replaced by this packet.

Event names are independently established by registration function `0x00608880`:
`0x006098E7` assigns `LFG_LOCK_INFO_RECEIVED` to `0x00C256B4`;
`0x006098F1` assigns `LFG_UPDATE_RANDOM_INFO` to `0x00C256B8`.
Event setup `0x0052A980` calls `0x0081B5F0` with table base `0x00C24EB0`;
these offsets are indices `0x201` and `0x202`. Dispatcher `0x0081B530` delegates
to `0x0081AC90`; null format means no payload. The latter invokes registered
handlers synchronously before returning. Consequently batched replies must not
be collapsed into one final snapshot followed by one event pair: each receipt's
published state must be observable to its own two notifications.

## Other native invalidation/update paths

- Full initialization `0x0055D440` clears self validity/locks/previews and party
  validity/records, then registers LFG packet handlers. Caller: `0x0052B550`
  (call `0x0052B85A`), reached through world setup `0x00405540`.
- Full teardown `0x0055AF20` clears those caches and unregisters handlers.
  Caller `0x00529160` (call `0x00529208`), reached from world teardown
  `0x00406510`, whose branches go to login/character selection.
- `LFG_DISABLED` (`0x0398`), branch `0x0055C6E0`, clears self locks/previews
  and party locks, as well as other LFG caches. Do not fabricate a successful
  empty lock reply or its normal event pair for this invalidation.
- Party removal helper `0x0055B120` calls party reset `0x0055AE20` only;
  it also clears role-check/proposal state when applicable. Callers include
  group paths `0x006CBE10`, `0x006CBE50`, `0x006CBEC0`. Self locks/previews
  remain untouched. This is **not** evidence to clear on every generic
  `PARTY_MEMBERS_CHANGED` notification or on each member-name update.
- `LFG_JOIN_RESULT` (`0x0364`) with result 6 clears/replaces the party lock block
  and marks party valid (write `0x0055CCBC`). It does not run the normal
  PLAYER_INFO/PARTY_INFO event pair at that point. This is a cross-unit L2→L1
  cache update seam, not permission for two competing reducers.
- `GetLFDLockInfo` (`0x00557930`) gates self index 1 on self validity and
  other indices on party validity. Invalid/missing player returns no values;
  valid player with absent lock reason returns name plus zero. Invalid and
  explicit empty therefore cannot share a single default-empty cache.

## ReloadUI is not session teardown

`ReloadUI` registration `0x00AC80C0/4` points to `0x005222E0`. Its normal path
sets flag `0x00BD0791`. The UI update function `0x0052AF40` sees that flag and
runs UI teardown `0x00528F00`, then UI construction `0x0052A980`.

The teardown calls in-world UI detach `0x00528C30`, resetting guard
`0x00BD0792` to zero; the next attach `0x00528010` sets it to one.
Neither of these LFG-relevant paths calls the full LFG reset/unregistration
`0x0055AF20` or initialization `0x0055D440`. The explicit self-validity writer
cross-references enumerate full initialization/teardown, disabled, and incoming
PLAYER_INFO replacement, **not** a ReloadUI cache reset. Party reset call sites
likewise distinguish party removal and reply replacement from UI recreation.
Thus replacement Lua state must be seeded with current native caches, without
replaying old packet events as if newly received.

There is, however, a real native re-query on UI attach: `0x00528010` calls
`0x00557240` (call `0x00528267`). That helper clears proposal-tracking word
`0x00BEAA18`, sends `GET_STATUS` (`0x0296`) via `0x00555920`, then sends
player lock request (`0x036E`) via `0x00555990`. This bypasses the Lua request
throttle; it does **not** clear existing lock/preview validity. UI construction
calls attach when a self GUID exists, including normal in-world ReloadUI.
There is no corresponding automatic party-lock sender in this traced path.
Do not mistake "do not replay queue decisions on ReloadUI" for "send no native
read-only refresh queries".

UI detach also calls `0x00558F90` (via a thunk), which leaves an active raid
browser search (`0x035F`) if its browser entry is nonzero. That is a separate
L5 lifecycle effect, not a lock-cache clear.

## Contract implications and remaining integration work

1. Keep separate optional self/preview and party snapshots. `None` means invalid;
   a received empty packet is valid empty. Preserve all raw ordered packet fields
   in the App snapshot; derived UI lookups need not erase duplicate/raw data.
2. Explicit accepted requests retain the last authoritative snapshot. Native
   boolean return/throttle belongs at the synchronous UI intent boundary, with
   player and party gates independent; no extra App deduplication.
3. Seed a new VM from current snapshots without receipt events. Implement native
   attach refresh separately from explicit Lua request throttling and preserve
   GET_STATUS→player-lock send order through Wire-owned commands.
4. For each arrival, install its replacement state and derived inputs before its
   ordered no-argument event pair. Event replay must be bounded/acknowledged,
   epoch-safe, and must not survive teardown into a different session.
5. Completion PLAYER_REWARD remains a separate L4 cache; preview `done` is not a
   completion event or cache.
6. Before App implementation: agree UI DTO, synchronous throttle clock ownership,
   receipt/setter/event seam, fresh-VM detection and native attach trigger. Map
   exact existing App group-removal/session transitions to the audited native
   paths rather than adding broad guessed invalidation hooks. L2 must own no
   competing capture and must route result-6 party updates into the same reducer.

Limits: this is static native control-flow evidence, not a live-session replay.
It does not establish a timeout, reply correlation, arbitrary TTL, zone-triggered
cache expiry, or invalidation on every group change. No such policy is authorized.
