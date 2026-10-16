# Approved L1 lock and preview boundary

UI/App jointly froze this boundary and the manager approved it before edits.
The first unit adds data types and side-effect-free setters only. It neither
replaces Lua compatibility bindings nor activates an incomplete App feed.
App integration and native getters land as coordinated follow-ups.

## Fields and ownership

UI owns `WrathLfgLockSnapshot`: independent optional player and party blocks.
`None` means invalid; a present empty block means a valid empty receipt.
Player records preserve ordered locks and previews; party records preserve full
GUIDs and ordered locks. Locks contain full packed entry and raw reason. Previews
contain entry, raw done byte, money/XP base and variance, and ordered item rows
(item ID, display ID, count). Preserve duplicates and unsigned bits.

The separate `WrathLfgPresentation` contains self GUID and keyed GUID names,
display icons and reward links. Missing keys are unavailable; present empty names
are retained. Existing item templates own names/quality, and existing L0 catalogs
own dungeon metadata and expansion context. Do not add a parallel item cache.

`set_wrath_lfg_lock_snapshot` and `set_wrath_lfg_presentation` accept immutable
shared `Arc` inputs. Neither emits events, sends requests nor invalidates another
input. App owns each receipt: publish snapshot and derived inputs before the
synchronous no-argument lock-info event followed by the random-info event. New
VMs seed current inputs without replaying receipts. Completion rewards remain L4.

## Derived semantics

Native full-entry nodes retain first insertion order. Duplicate locks overwrite
the reason; duplicate previews overwrite scalar fields and append items. Activity
is explicit lock-block presence, not a nonzero reason. Distinct packed types do
not collide. Best random choice uses existing catalog/expansion and lock inputs;
its signed strict comparisons retain the earlier candidate on an exact tie.

Reward names require the existing item-template cache (empty string until ready).
Icons use native path prefix 3 and the display-ID filename, falling back to
`INV_Misc_QuestionMark`. Links use the shared App Wrath nine-field formatter with
zero enhancements/random/suffix, cached template and semantic player level,
**or level 0 when the native self object is unavailable**. This native fallback
does not change the mail wrapper's existing availability guard.

## Requests and item callbacks (follow-up implementation)

UI owns independent synchronous process-lifetime player/party request gates,
retained across VM/session resets. App supplies an injectable OS-monotonic u32
millisecond clock, preserving native conversion/float quantization. Requests do
not invalidate caches. Attach sends GET_STATUS then player-lock refresh outside
that explicit gate. No pending state, TTL or fabricated correlation.

Keep `on_item -> Items::insert_template` as sole item answer owner and
`feed_item_stats -> set_item_template` as sole destructive fresh-ID drain.
An ordered bounded/acknowledged epoch/sequence completion FIFO preserves answer
order separately from non-deduplicated getter callback interests. Cache-ready
getters register no interest. Publish presentation before one lone random-info
event per successful matching registration. Failure/eviction drains matching
interests without events; unrelated answers do nothing. No lock-validity gate.

**Approved host safety policy, not proven native behavior:** interests survive
VM replacement within a native-session epoch, but disconnect/character changes
cancel old interests. Native cross-session cancellation was not established.
Transport request deduplication must not coalesce callback multiplicity.

## Evidence

Installed build-12340 binary SHA256
`c6f9209840a8a5941538bcda0e63c4bf445a154b6c1c9c7397be31cebabc289c`.
Native getters 0x557930/0x557890/0x5576b0/0x557aa0/0x557be0/0x557d60;
shared nodes 0x559840/0x559b10; random choice 0x559ea0/0x559da0;
item callback 0x552950, registration 0x67ca30, completion 0x67cbd0,
per-item failure 0x67a070; link builder 0x61e360/0x61e290.
See `WRATH-LFG-L1-LIFECYCLE.md` for lifecycle evidence. Local read-only audit logs
are `/tmp/azb-ui-lfg-l1-*.log`; no executable bytes or stock sources committed.
