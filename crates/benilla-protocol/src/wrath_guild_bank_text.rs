//! Native bank text and log requests: 0x005a5940/0x005a5c40 send byte tabs;
//! 0x005a4ae0 reads byte tab then a terminated string. UI cache sizes and
//! text normalization are deliberately not wire-schema limits here.
use std::io;

pub const MSG_GUILD_BANK_LOG_QUERY: u32 = 0x03ee;
pub const MSG_GUILD_BANK_QUERY_TEXT: u16 = 0x040a;
pub const CMSG_GUILD_BANK_SET_TAB_TEXT: u32 = 0x040b;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    pub tab: u8,
    /// Raw bytes excluding the terminator; no lossy UTF-8 conversion.
    pub text: Vec<u8>,
}

pub fn decode(body: &[u8]) -> io::Result<Text> {
    let (&tab, rest) = body
        .split_first()
        .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
    let end = rest
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
    if end + 1 != rest.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "trailing guild bank text bytes",
        ));
    }
    Ok(Text {
        tab,
        text: rest[..end].to_vec(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    QueryLog { tab: u8 },
    QueryText { tab: u8 },
    SetText(Text),
}

impl Command {
    pub const fn opcode(&self) -> u32 {
        match self {
            Self::QueryLog { .. } => MSG_GUILD_BANK_LOG_QUERY,
            Self::QueryText { .. } => MSG_GUILD_BANK_QUERY_TEXT as u32,
            Self::SetText(_) => CMSG_GUILD_BANK_SET_TAB_TEXT,
        }
    }

    pub fn encode(&self) -> io::Result<Vec<u8>> {
        match self {
            Self::QueryLog { tab } | Self::QueryText { tab } => Ok(vec![*tab]),
            Self::SetText(Text { tab, text }) => {
                if text.contains(&0) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "embedded guild bank text NUL",
                    ));
                }
                let mut body = vec![*tab];
                body.extend(text);
                body.push(0);
                Ok(body)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_preserves_raw_tab_bytes_and_explicit_empty() {
        assert_eq!(
            decode(&[255, 0]).unwrap(),
            Text {
                tab: 255,
                text: vec![]
            }
        );
        assert_eq!(
            decode(&[5, b'A', 255, 128, b'\n', 0]).unwrap(),
            Text {
                tab: 5,
                text: vec![b'A', 255, 128, b'\n']
            }
        );
        let body = [5, b'A', 255, 128, b'\n', 0];
        for len in 0..body.len() {
            assert!(decode(&body[..len]).is_err());
        }
        assert!(decode(&[0, 0, 1]).is_err());
        assert!(decode(&[0, 0, 0]).is_err());
    }

    #[test]
    fn requests_are_byte_tab_and_setter_is_raw_terminated_text() {
        for tab in [0, 5, 6, 255] {
            for (cmd, opcode) in [
                (Command::QueryLog { tab }, 0x03ee),
                (Command::QueryText { tab }, 0x040a),
            ] {
                assert_eq!(cmd.opcode(), opcode);
                assert_eq!(cmd.encode().unwrap(), [tab]);
            }
        }
        let cmd = Command::SetText(Text {
            tab: 255,
            text: vec![b'A', 255],
        });
        assert_eq!(cmd.opcode(), 0x040b);
        assert_eq!(cmd.encode().unwrap(), [255, b'A', 255, 0]);
        assert_eq!(
            Command::SetText(Text {
                tab: 0,
                text: vec![]
            })
            .encode()
            .unwrap(),
            [0, 0]
        );
        assert!(Command::SetText(Text {
            tab: 0,
            text: vec![b'A', 0, b'B']
        })
        .encode()
        .is_err());
    }

    #[test]
    fn common_reply_decoder_delegates_without_partial_acceptance() {
        assert_eq!(
            crate::wrath_guild_bank::decode(0x040a, &[3, 255, 0]).unwrap(),
            crate::wrath_guild_bank::Packet::Text(Text {
                tab: 3,
                text: vec![255]
            })
        );
        assert!(crate::wrath_guild_bank::decode(0x040a, &[3, 255]).is_err());
    }
}
