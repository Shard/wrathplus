# Character-screen services

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `CharacterHandler.cpp`
(`HandleCharRenameOpcode`, `HandleCharCustomize`, `HandleCharFactionOrRaceChange`,
`HandleSetPlayerDeclinedNames` and their `Send*` replies) and `MiscHandler.cpp`
(`HandleRealmSplitOpcode`). No client-only behaviour is inferred; no Ghidra round. Codec and session
method: `crates/wrathplus-protocol/src/wrath_char_services.rs`.

These run on the pre-world session at character select, like create and delete:
`CharRequest::Service(Command)` → `WrathWorldSession::char_service_result`, which sends the request
and waits (bounded, Warden-aware) for its reply. A reply that applied a change (rename, customize,
faction or race change with result 0) re-enumerates the roster and publishes `CharacterList`
**before** `SessionEvent::WrathCharServiceResult { reply }`, the same roster-before-result order as
create/delete. Codes are raw `ResponseCodes`.

- 02C7 → 02C8 rename (GUID, name; success echoes both).
- 0473 → 0474 customize (GUID, name, gender, skin, hair colour, hair style, facial hair, face; the
  success reply orders gender, skin, face, hair style, hair colour, facial hair).
- 04D9 faction change and 04F8 race change → 04DA (the customize fields plus race; success adds race).
- 0419 → 041A declined names (base name and five cases; u32 result and GUID).
- 038C → 038B realm split (an echoed word; state and date).

**Status**: requests transport-only (App Migration wires the char-screen producers and the result
consumer); replies capture-only in `NativeCharServiceReplies` until then. 1.12 sessions ignore the
request with a warning.
