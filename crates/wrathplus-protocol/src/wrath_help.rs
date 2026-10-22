//! Build-12340 Help Request: GM tickets, the GM response and survey, complaints and bug reports.
//! Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03: TicketHandler.cpp
//! (create, update, delete, get, system status, survey, response resolve), TicketMgr.cpp
//! (`GmTicket::WritePacket`, `GmTicket::SendResponse`, `TicketMgr::SendTicket`), MiscHandler.cpp
//! (`HandleBugOpcode`, `HandleComplainOpcode`), MiscPackets.cpp (`Complain`, `ComplainResult`),
//! TicketHandler.cpp (`HandleReportLag`).
//! Strict framing.
//!
//! The ticket replies mean what 1.12's do and feed the shared events. This build's ticket has no
//! category (0) and adds a ticket id and a needs-more-help byte (dropped); its escalation and
//! viewed bytes are 1.12's assigned and opened bytes. The GM response, the survey-offer status and
//! the complaint receipt have no 1.12 counterpart and are capture-only.
use crate::types::GmTicket;
use crate::SessionEvent;
use std::io::{self, Cursor, Read};

pub const OPCODES: [u16; 8] = [
    0x0206, 0x0208, 0x0212, 0x0218, 0x021b, 0x03c8, 0x04ef, 0x04f1,
];

/// `GMTICKET_STATUS_HASTEXT`: an open ticket follows.
const STATUS_HAS_TEXT: u32 = 0x06;
/// `GMTICKET_STATUS_DEFAULT`: no ticket.
const STATUS_DEFAULT: u32 = 0x0a;

#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    /// `SMSG_GMRESPONSE_RECEIVED`: a GM's answer to the open ticket. AC splits the response into
    /// four NUL-terminated parts of up to 3,999 bytes, empty ones included.
    GmResponse {
        response_id: u32,
        ticket_id: u32,
        ticket_text: String,
        response: [String; 4],
    },
    /// `SMSG_GMRESPONSE_STATUS_UPDATE`: sent on resolving a response; 1 offers the GM survey.
    GmResponseStatus { show_survey: u8 },
    /// `SMSG_COMPLAIN_RESULT`: the complaint was received; AC always sends 0.
    ComplainResult { raw: u8 },
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

fn u8_(r: &mut Cursor<&[u8]>) -> io::Result<u8> {
    let mut b = [0; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn u32_(r: &mut Cursor<&[u8]>) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn f32_(r: &mut Cursor<&[u8]>) -> io::Result<f32> {
    u32_(r).map(f32::from_bits)
}

/// A NUL-terminated string; the terminator is required.
fn cstring(r: &mut Cursor<&[u8]>) -> io::Result<String> {
    let rest = &r.get_ref()[r.position() as usize..];
    let end = rest
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| invalid("unterminated string"))?;
    let text = String::from_utf8_lossy(&rest[..end]).into_owned();
    r.set_position(r.position() + end as u64 + 1);
    Ok(text)
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<SessionEvent> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let capture = |packet| SessionEvent::WrathHelp { packet };
    let event = match opcode {
        0x0206 => SessionEvent::GmTicketCreated { response: u32_(r)? },
        0x0208 => SessionEvent::GmTicketUpdated { response: u32_(r)? },
        0x0218 => SessionEvent::GmTicketDeleted { response: u32_(r)? },
        // AC writes `GMTICKET_QUEUE_STATUS_ENABLED`/`DISABLED` as a u32; signed as 1.12 reads it.
        0x021b => SessionEvent::GmTicketSystemStatus {
            status: u32_(r)? as i32,
        },
        0x0212 => match u32_(r)? {
            STATUS_DEFAULT => SessionEvent::GmTicket { ticket: None },
            STATUS_HAS_TEXT => {
                let _ticket_id = u32_(r)?;
                let text = cstring(r)?;
                let _need_more_help = u8_(r)?;
                let (ticket_age, oldest_ticket_age, update_time) = (f32_(r)?, f32_(r)?, f32_(r)?);
                let (assigned_to_gm, opened_by_gm) = (u8_(r)?, u8_(r)?);
                SessionEvent::GmTicket {
                    ticket: Some(Box::new(GmTicket {
                        text,
                        category: 0,
                        ticket_age,
                        oldest_ticket_age,
                        update_time,
                        assigned_to_gm,
                        opened_by_gm,
                    })),
                }
            }
            _ => return Err(invalid("GM ticket status")),
        },
        0x04ef => capture(Packet::GmResponse {
            response_id: u32_(r)?,
            ticket_id: u32_(r)?,
            ticket_text: cstring(r)?,
            response: [cstring(r)?, cstring(r)?, cstring(r)?, cstring(r)?],
        }),
        0x04f1 => capture(Packet::GmResponseStatus {
            show_survey: u8_(r)?,
        }),
        0x03c8 => capture(Packet::ComplainResult { raw: u8_(r)? }),
        _ => return Err(invalid("help request opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("help request extent"));
    }
    Ok(event)
}

/// One answered survey question: a `GMSurveyQuestions` id (nonzero), its rank and a comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurveyAnswer {
    pub question: u32,
    pub rank: u8,
    pub comment: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// `CMSG_GMTICKET_CREATE`: where the player stands, the text and the two response flags. No
    /// chat transcript is attached (count 0, size 0).
    Create {
        map: u32,
        pos: [f32; 3],
        text: String,
        need_response: bool,
        need_more_help: bool,
    },
    /// `CMSG_GMTICKET_UPDATETEXT`.
    UpdateText { text: String },
    /// `CMSG_GMTICKET_GETTICKET`: AC answers with the server time, then the ticket.
    GetTicket,
    /// `CMSG_GMTICKET_DELETETICKET`.
    DeleteTicket,
    /// `CMSG_GMTICKET_SYSTEMSTATUS`.
    SystemStatus,
    /// `CMSG_GMSURVEY_SUBMIT`: a `GMSurveyCurrentSurvey` id, up to ten answers and a comment.
    SurveySubmit {
        survey: u32,
        answers: Vec<SurveyAnswer>,
        comment: String,
    },
    /// `CMSG_GMRESPONSE_RESOLVE`: the player read the GM's response.
    ResolveResponse,
    /// `CMSG_COMPLAIN` about a mail.
    ComplainMail { spammer: u64, mail_id: u32 },
    /// `CMSG_COMPLAIN` about a chat line: its language, message type, channel, age and a
    /// description AC stores verbatim.
    ComplainChat {
        spammer: u64,
        language: u32,
        message_type: u32,
        channel_id: u32,
        seconds_since: u32,
        description: String,
    },
    /// `CMSG_GM_REPORT_LAG` (`GMReportLag`): the lag type and where the player stands.
    ReportLag {
        lag_type: u32,
        map: u32,
        pos: [f32; 3],
    },
    /// `CMSG_BUG`: a bug report or a suggestion. AC ignores both length words.
    Bug {
        suggestion: bool,
        content: String,
        kind: String,
    },
}

fn put_str(text: &str, body: &mut Vec<u8>) -> io::Result<()> {
    if text.as_bytes().contains(&0) {
        return Err(invalid("interior NUL"));
    }
    body.extend(text.as_bytes());
    body.push(0);
    Ok(())
}

/// The packet for `command`, refused before send when a string carries a NUL or a survey has more
/// than ten answers or a zero question (AC stops at the first zero).
pub fn encode(command: &Command) -> io::Result<(u32, Vec<u8>)> {
    let mut body = Vec::new();
    let opcode = match command {
        Command::Create {
            map,
            pos,
            text,
            need_response,
            need_more_help,
        } => {
            body.extend(map.to_le_bytes());
            for v in pos {
                body.extend(v.to_le_bytes());
            }
            put_str(text, &mut body)?;
            body.extend(u32::from(*need_response).to_le_bytes());
            body.push(u8::from(*need_more_help));
            // Chat-log time count and decompressed size: no transcript.
            body.extend(0u32.to_le_bytes());
            body.extend(0u32.to_le_bytes());
            0x0205
        }
        Command::UpdateText { text } => {
            put_str(text, &mut body)?;
            0x0207
        }
        Command::GetTicket => 0x0211,
        Command::DeleteTicket => 0x0217,
        Command::SystemStatus => 0x021a,
        Command::SurveySubmit {
            survey,
            answers,
            comment,
        } => {
            if answers.len() > 10 || answers.iter().any(|a| a.question == 0) {
                return Err(invalid("survey answers"));
            }
            body.extend(survey.to_le_bytes());
            for a in answers {
                body.extend(a.question.to_le_bytes());
                body.push(a.rank);
                put_str(&a.comment, &mut body)?;
            }
            if answers.len() < 10 {
                body.extend(0u32.to_le_bytes());
            }
            put_str(comment, &mut body)?;
            0x032a
        }
        Command::ResolveResponse => 0x04f0,
        Command::ReportLag { lag_type, map, pos } => {
            body.extend(lag_type.to_le_bytes());
            body.extend(map.to_le_bytes());
            for v in pos {
                body.extend(v.to_le_bytes());
            }
            0x0502
        }
        Command::ComplainMail { spammer, mail_id } => {
            body.push(0);
            body.extend(spammer.to_le_bytes());
            body.extend(0u32.to_le_bytes());
            body.extend(mail_id.to_le_bytes());
            body.extend(0u32.to_le_bytes());
            0x03c7
        }
        Command::ComplainChat {
            spammer,
            language,
            message_type,
            channel_id,
            seconds_since,
            description,
        } => {
            body.push(1);
            body.extend(spammer.to_le_bytes());
            for v in [language, message_type, channel_id, seconds_since] {
                body.extend(v.to_le_bytes());
            }
            put_str(description, &mut body)?;
            0x03c7
        }
        Command::Bug {
            suggestion,
            content,
            kind,
        } => {
            body.extend(u32::from(*suggestion).to_le_bytes());
            body.extend((content.len() as u32).to_le_bytes());
            put_str(content, &mut body)?;
            body.extend((kind.len() as u32).to_le_bytes());
            put_str(kind, &mut body)?;
            0x01ca
        }
    };
    Ok((opcode, body))
}

#[cfg(test)]
mod tests;
