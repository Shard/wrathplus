# Refer-a-friend level grants

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `ReferAFriendHandler.cpp`
(`HandleGrantLevel`, `HandleAcceptGrantLevel`), `Player.h` (`ReferAFriendError`). No client-only
behaviour is inferred; no Ghidra round. Codec: `crates/benilla-protocol/src/wrath_refer_friend.rs`.

**Capture-only** (`NativeReferFriendPackets`, sole owner, cleared on Connected and Disconnected,
duplicates kept): 041F propose level grant (the granter's packed guid, which the accept echoes),
0421 failure (a `ReferAFriendError` u32; only `ERR_REFER_A_FRIEND_NOT_IN_GROUP` (9) appends the
target's name, any other code with a name is malformed). 1.12 has no level grant, so no shared
event means these.

**Outbound**, transport-only `ClientCommand::NativeReferFriend` (no UI producer yet): 040D grant
level (target's packed guid), 0420 accept level grant (granter's packed guid). Routes need the live
writer only; AC silently ignores an accept from anyone but the recruiter.
