# Help Request: GM tickets, response, survey, complaints and bug reports

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `TicketHandler.cpp`
(create, update, delete, get, system status, survey submit, response resolve), `TicketMgr.cpp`
(`GmTicket::WritePacket`, `GmTicket::SendResponse`, `TicketMgr::SendTicket`), `MiscHandler.cpp`
(`HandleBugOpcode`, `HandleComplainOpcode`), `MiscPackets.cpp` (`Complain`, `ComplainResult`).
No client-only behaviour is inferred; no Ghidra round. Codec:
`crates/benilla-protocol/src/wrath_help.rs`.

**Shared (handled)**, fed to the existing `ui_gm_ticket` flow: 0206 create, 0208 update text,
0218 delete (each a u32 response code), 021B system status (u32, read signed as 1.12 does), 0212
get ticket. A 0212 status of `GMTICKET_STATUS_DEFAULT` (0x0A) is "no ticket"; `HASTEXT` (0x06)
carries the ticket id, text, needs-more-help byte, three ages in days, the escalation byte and the
viewed byte. This build has no ticket category, so the shared ticket's category is 0; the escalation
and viewed bytes are 1.12's assigned and opened bytes; the ticket id and needs-more-help byte are
dropped. Any other status is malformed.

**Capture-only** (`NativeHelpPackets`, sole owner, cleared on Connected and Disconnected,
duplicates kept): 04EF GM response (response id, ticket id, ticket text and the response's four
NUL-terminated parts, empty ones kept), 04F1 response status (1 offers the GM survey), 03C8
complaint receipt (AC always 0). 1.12 has no reply channel or survey offer, so no shared event
means these.

**Outbound**: the existing ticket commands now have native writers (route `NativeHelp`, producer
`ui_gm_ticket`): 0205 create (map, position, text, `needResponse` 1, `needMoreHelp` 0, no chat
transcript: count 0 and size 0; the 1.12 category is dropped), 0207 update text (the category is
dropped), 0211 get ticket, 0217 delete, 021A system status. Transport-only
`ClientCommand::NativeHelp`: 032A survey submit (survey id, up to ten nonzero question/rank/comment
triples, a zero terminator below ten, then the overall comment), 04F0 response resolve, 03C7
complaint (mail form: type 0, spammer, 0, mail id, 0; chat form: type 1, spammer, language,
message type, channel, seconds since, description), 01CA bug report (suggestion flag, then each
string with a length word AC ignores; the writer sends its byte length). A string with an interior
NUL or a malformed survey is refused before send. Routes need the live writer only; no server
acceptance is claimed.

Not in this unit: Lua bindings for the 3.3.5 Help frame (`GMResponseResolve`, the survey frame,
`ReportPlayer`), which belong to the UI owners; the transport-only commands have no producer yet.
