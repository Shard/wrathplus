# Wrath inbox item links (build 12340)

Stock `MailFrame.xml:353` passes the message index and attachment slot to
`GetInboxItemLink`. The Wrath-only binding reads `WrathMailAttachment::link`;
the app owns link construction and refresh. `None` means unavailable, not a
placeholder link. The initial seam leaves app links unavailable pending projection.

Native binding `0x56eca0` requires a numeric first argument (numeric strings
accepted), truncates it and subtracts one. The second argument instead subtracts
one *before* truncation, consuming the low signed 32 bits. Omitted/nonnumeric or
result `-1` selects the first occupied raw slot 0–15, independent of packet order.
An occupied unresolved slot is not skipped. Invalid message/slot, entry zero or
unavailable metadata returns **zero Lua values**, not one nil.

App projection contract (native builder `0x61e290`):
`|Hitem:ID:enchant:gem1:gem2:gem3:0:randomProperty:suffixFactor:playerLevel|h[name]|h`
with native quality color/reset. Integer fields use signed i32 formatting;
quality >7 clamps to 1. Enchant IDs are raw attachment indices 0,2,3,4; fourth gem
is literal zero. Name includes the resolved random property/suffix. Level is the
current semantic player level, refreshed when it changes; do not substitute an
unknown level with zero or reuse Vanilla's four-field formatter.

Evidence: installed client SHA256
`c6f9209840a8a5941538bcda0e63c4bf445a154b6c1c9c7397be31cebabc289c`,
mail parser `0x571c50`, first occupied selector `0x56d420`, name builder `0x706d70`.
