# Session odds and ends

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `QueryHandler.cpp`
(`SendTimeQueryResponse`), `QueryPackets.cpp` (`TimeQueryResponse`), `MiscPackets.cpp` (mirror
timers, `PlayTimeWarning`, `CrossedInebriationThreshold`), `CharacterHandler.cpp` (feature-system
status at login), `ChatHandler.cpp` (`HandleEmoteOpcode`), `SkillHandler.cpp`
(`HandleUnlearnSkillOpcode`), `MiscHandler.cpp` (`HandlePlayerLogoutOpcode`), `WorldSession.cpp`
(`SendAddonsInfo`), `AuthHandler.cpp` (`SendClientCacheVersion`). No client-only behaviour is
inferred; no Ghidra round. Codec: `crates/benilla-protocol/src/wrath_session_misc.rs`. Not in
`wrath_session_meta` (Wire B's).

**Shared (handled)**: 01CF time response (server unix time to `ServerUnixTime`, the server
wall clock), 01D9/01DA/01DB mirror timer start, pause and stop (the 1.12 layout; the paused byte
must be 0 or 1). The time response's second word, seconds until the daily quest reset, is new in
this build and is also captured.

**Capture-only** (`NativeSessionMiscPackets`, sole owner, cleared on Connected and Disconnected,
duplicates kept): the 01CF daily-reset countdown, 02F5 play-time warning (`PlayTimeFlag`, signed
seconds left), 03C1 crossed inebriation threshold (player, threshold, item), 03C9 feature-system
status (complaint mode, voice-chat switch; AC sends 2 and 0 after login).

**Outbound**: the existing `QueryTime` (the server-clock resync in `net`) and `UnlearnSkill`
(`ui_char`) commands now route natively (01CE empty, 0202 skill id); `QueryTime` was previously a
disabled game action, and the route tests now expect the native route. Transport-only
`ClientCommand::NativeSessionMisc`: 004A player logout (empty; AC's handler does nothing, logging out
is `CMSG_LOGOUT_REQUEST`), 0102 emote (AC acts only on 0 and 3, wave).

**Bootstrap packets (Wire A, follow-up)**: 02EF addon info and 04AB client-cache version arrive
right after authentication, before `CMSG_PLAYER_LOGIN`. `wrath_world::WrathWorldSession` now keeps
both through its startup, roster and character-action loops, which used to discard them. It also
remembers that the addon reply arrived, across re-logins on the session. At login, `net/io/wrath.rs`
publishes them after `Connected` as `SessionEvent::WrathBootstrap` (`wrath_bootstrap.rs`, strict)
into `net/wrath_bootstrap_capture.rs`.
- **02EF:** handled. Our auth session declares no addons, so the packet is the banned-addon list
  alone. `Connected.addon_info` is now the truth: an empty hidden list once the reply arrived,
  `None` if it never did. Matching the banned MD5s against loaded addons is not built.
- **04AB:** capture-only. There is no persistent `WDB` cache here for it to invalidate.
