//! The one write point every in-world client packet passes. [`LoggedWriter`] owns the split Wrath
//! writer, so a route cannot frame a packet without its line: `net(wrath) SEND 0x<op> <len>
//! <name>` at info, then up to [`BODY_SHOWN`] body bytes. The pre-world session's traffic (auth,
//! character list) stays inside the protocol session; the auth proof and Warden replies, should
//! one reach here, log their length only.

use wrathplus_protocol::wrath_world::WrathWorldWriter;

/// `CMSG_AUTH_SESSION` (the session-key proof) and `CMSG_WARDEN_DATA`: never a body in the log.
const LENGTH_ONLY: [u32; 2] = [0x01ED, 0x02E7];

/// The body bytes a line shows, so a movement heartbeat stays one short line.
const BODY_SHOWN: usize = 64;

/// The split writer behind the one logged `send_raw`.
pub(super) struct LoggedWriter(WrathWorldWriter);

impl LoggedWriter {
    pub(super) fn new(writer: WrathWorldWriter) -> Self {
        Self(writer)
    }

    /// Log the packet, then frame and send it. A failure here is the socket's, tagged
    /// [`SendFailed`] so the writer loop ends the session on it and on nothing a route encodes.
    pub(super) fn send_raw(&mut self, opcode: u32, body: &[u8]) -> anyhow::Result<()> {
        use anyhow::Context;
        bevy::log::info!("{}", send_line(opcode, body));
        self.0.send_raw(opcode, body).context(SendFailed)
    }
}

/// The context every [`LoggedWriter::send_raw`] error carries: the socket write failed.
#[derive(Debug)]
pub(super) struct SendFailed;

impl std::fmt::Display for SendFailed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Wrath socket send failed")
    }
}

/// `0x<op> <name>`, for a line about a packet that never reached the socket.
pub(super) fn opcode_label(opcode: u32) -> String {
    let name = u16::try_from(opcode)
        .ok()
        .and_then(wrathplus_protocol::wrath_opcode_names::opcode_name)
        .unwrap_or("?");
    format!("0x{opcode:04x} {name}")
}

/// The line [`LoggedWriter::send_raw`] logs.
fn send_line(opcode: u32, body: &[u8]) -> String {
    let (label, len) = (opcode_label(opcode), body.len());
    let (hex, name) = label.split_once(' ').unwrap_or((&label, "?"));
    let mut line = format!("net(wrath) SEND {hex} {len} {name}");
    if !LENGTH_ONLY.contains(&opcode) {
        let shown = &body[..body.len().min(BODY_SHOWN)];
        line.push_str(&format!(" {shown:02x?}"));
        if body.len() > BODY_SHOWN {
            line.push_str(" …");
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_names_the_opcode_and_length_and_keeps_secrets_out() {
        use wrathplus_protocol::wrath_opcode_names::opcode_name;
        assert_eq!(
            send_line(0x0116, &[1, 2]),
            "net(wrath) SEND 0x0116 2 CMSG_INITIATE_TRADE [01, 02]"
        );
        assert_eq!(opcode_name(0x01ED), Some("CMSG_AUTH_SESSION"));
        assert_eq!(opcode_name(0x02E7), Some("CMSG_WARDEN_DATA"));
        for opcode in LENGTH_ONLY {
            assert!(
                !send_line(opcode, &[0xAB; 20]).contains("ab"),
                "{opcode:#x}"
            );
        }
        let long = send_line(0x0095, &[7; 100]);
        assert!(long.starts_with("net(wrath) SEND 0x0095 100 CMSG_MESSAGECHAT [07"));
        assert!(long.ends_with(" …") && long.matches("07").count() == BODY_SHOWN);
    }

    /// Every route writes through the swap's `writer`, and that field is the logged writer: no
    /// `send_raw` in the writer thread reaches the protocol writer any other way.
    #[test]
    fn every_writer_route_sends_through_the_logged_point() {
        let source = include_str!("wrath.rs");
        let code = source.split("\n#[cfg(test)]").next().unwrap();
        assert!(code.contains("    writer: super::wrath_send_log::LoggedWriter,\n"));
        assert!(!code.contains("writer: WrathWorldWriter"));
        let mut calls = 0;
        for (at, _) in code.match_indices(".send_raw(") {
            // `active.writer.send_raw(` or a chain ending `.writer` on the line above.
            let receiver = code[..at].trim_end().rsplit(char::is_whitespace).next();
            assert!(
                receiver.is_some_and(|r| r.ends_with("writer")),
                "a send_raw not on the logged writer, after {receiver:?}"
            );
            calls += 1;
        }
        assert!(calls > 40, "the writer thread's sends were found ({calls})");
    }
}
