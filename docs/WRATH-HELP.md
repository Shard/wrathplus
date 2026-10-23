# Help Request: GM tickets, response, survey, complaints and bug reports

Wire reference: AzerothCore `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: `TicketHandler.cpp`
(create, update, delete, get, system status, survey submit, response resolve), `TicketMgr.cpp`
(`GmTicket::WritePacket`, `GmTicket::SendResponse`, `TicketMgr::SendTicket`), `MiscHandler.cpp`
(`HandleBugOpcode`, `HandleComplainOpcode`), `MiscPackets.cpp` (`Complain`, `ComplainResult`).
No client-only behaviour is inferred; no Ghidra round. Codec:
`crates/wrathplus-protocol/src/wrath_help.rs`.

**Shared (handled)**, fed to the existing `ui_gm_ticket` flow: 0206 create, 0208 update text,
0218 delete (each a u32 response code), 021B system status (u32, read signed as 1.12 does), 0212
get ticket. A 0212 status of `GMTICKET_STATUS_DEFAULT` (0x0A) is "no ticket"; `HASTEXT` (0x06)
carries the ticket id, text, needs-more-help byte, three ages in days, the escalation byte and the
viewed byte. This build has no ticket category, so the shared ticket's category is 0; the escalation
and viewed bytes are 1.12's assigned and opened bytes; the ticket id and needs-more-help byte are
dropped. Any other status is malformed.

**GM response** (`ui_gm_response`, the sole `WrathHelp` owner; 12340's dispatcher
`FUN_005ad240`): 04EF fires `GMRESPONSE_RECEIVED` (event `0x28f`, `"%s%s"`) with the ticket text
(2,000-byte buffer) and the four response parts joined (4,000 bytes each, 16,000 in all); 04F1
fires `GMSURVEY_DISPLAY` through `ui_gm_ticket`'s survey path for a nonzero byte, else a bare
`UPDATE_TICKET`. 03C8, the complaint receipt (AC always 0), is logged only: no UI effect traced.

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

**Help-frame globals** (`U/script/wrath_help_request.rs`, Wrath dialect only), queued with the
ticket verbs and sent by `ui_gm_ticket` with the player's map and position: `GMReportLag(number)`
(`0x5ad020`, rounded) sends 0502 `CMSG_GM_REPORT_LAG` (lag type, map, x, y, z; AC
`HandleReportLag`); `GMResponseNeedMoreHelp(text)` (`0x5ad150` → `0x5ac8f0(text, 1, 1)`) files a new
0205 ticket with the caller's text, asking for a response and flagged needs-more-help. Either
global's wrong argument type raises the native usage error. Untraced: the `0x5191c0(0x11)` UI-state
gate in front of the need-more-help send, which is not applied. 0502 was classified GM/debug; the
player Help frame sends it, so it is in scope.

Not in this unit: Lua bindings for `GMResponseResolve`, the survey frame and `ReportPlayer`, which
belong to the UI owners; those transport-only commands have no producer yet.
