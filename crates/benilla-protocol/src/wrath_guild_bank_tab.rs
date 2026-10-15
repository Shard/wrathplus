//! Native 0x005a5a00 sends GUID, byte tab, name and resolved icon strings.
//! Name normalization/validation and icon-index resolution belong to the caller;
//! this wire codec preserves supplied bytes and rejects embedded terminators.
use std::io;

pub const CMSG_GUILD_BANK_UPDATE_TAB: u32 = 0x03eb;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateTab {
    pub banker: u64,
    pub tab: u8,
    pub name: Vec<u8>,
    pub icon: Vec<u8>,
}

impl UpdateTab {
    pub const fn opcode(&self) -> u32 {
        CMSG_GUILD_BANK_UPDATE_TAB
    }

    pub fn encode(&self) -> io::Result<Vec<u8>> {
        if self.name.contains(&0) || self.icon.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "embedded guild bank tab NUL",
            ));
        }
        let mut body = self.banker.to_le_bytes().to_vec();
        body.push(self.tab);
        body.extend(&self.name);
        body.push(0);
        body.extend(&self.icon);
        body.push(0);
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_order_and_raw_strings() {
        let command = UpdateTab {
            banker: 0x0102030405060708,
            tab: 255,
            name: vec![b'A', 255],
            icon: vec![128, b'\\', b'B'],
        };
        assert_eq!(command.opcode(), 0x03eb);
        assert_eq!(
            command.encode().unwrap(),
            [8, 7, 6, 5, 4, 3, 2, 1, 255, b'A', 255, 0, 128, b'\\', b'B', 0]
        );
    }

    #[test]
    fn explicit_empty_strings_without_invented_eligibility_limits() {
        let mut command = UpdateTab {
            banker: 0,
            tab: 0,
            name: vec![],
            icon: vec![],
        };
        assert_eq!(command.encode().unwrap(), [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        command.name = vec![255; 256];
        command.icon = vec![128; 512];
        let body = command.encode().unwrap();
        assert_eq!(&body[9..265], &[255; 256]);
        assert_eq!(body[265], 0);
        assert_eq!(&body[266..778], &[128; 512]);
        assert_eq!(body[778], 0);
    }

    #[test]
    fn rejects_terminators_in_either_string() {
        for (name, icon) in [
            (vec![0], vec![]),
            (vec![], vec![0]),
            (vec![b'A', 0, b'B'], vec![b'C']),
            (vec![b'A'], vec![b'B', 0, b'C']),
        ] {
            assert_eq!(
                UpdateTab {
                    banker: 1,
                    tab: 5,
                    name,
                    icon
                }
                .encode()
                .unwrap_err()
                .kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }
}
