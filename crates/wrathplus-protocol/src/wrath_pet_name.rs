//! Build-12340 pet-name query. PetHandler.cpp::HandlePetNameQuery/SendPetNameQuery;
//! native response reader 006352c0. Strings are raw bytes, excluding wire NULs.
//! Capture only: empty replies and timestamps do not establish a refresh/retry policy.
use crate::wrath_session::{u32_le, u8_le};
use std::io::{self, Cursor};

pub const CMSG_PET_NAME_QUERY: u32 = 0x0052;
pub const SMSG_PET_NAME_QUERY_RESPONSE: u16 = 0x0053;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub pet_number: u32,
    pub name: Vec<u8>,
    pub timestamp: u32,
    /// Any nonzero byte introduces exactly five C strings; retain unknown values.
    pub declined_flag: u8,
    pub declined_names: Option<[Vec<u8>; 5]>,
}
fn cstring(r: &mut Cursor<&[u8]>) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        let b = u8_le(r)?;
        if b == 0 {
            return Ok(bytes);
        }
        bytes.push(b);
    }
}
pub fn decode(body: &[u8]) -> io::Result<Response> {
    let mut r = Cursor::new(body);
    let pet_number = u32_le(&mut r)?;
    let name = cstring(&mut r)?;
    let timestamp = u32_le(&mut r)?;
    let declined_flag = u8_le(&mut r)?;
    let declined_names = if declined_flag != 0 {
        Some([
            cstring(&mut r)?,
            cstring(&mut r)?,
            cstring(&mut r)?,
            cstring(&mut r)?,
            cstring(&mut r)?,
        ])
    } else {
        None
    };
    if r.position() != body.len() as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "trailing pet-name bytes",
        ));
    }
    Ok(Response {
        pet_number,
        name,
        timestamp,
        declined_flag,
        declined_names,
    })
}
pub fn encode_query(pet_number: u32, guid: u64) -> (u32, Vec<u8>) {
    let mut body = pet_number.to_le_bytes().to_vec();
    body.extend(guid.to_le_bytes());
    (CMSG_PET_NAME_QUERY, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_every_flag_raw_strings_and_missing_vs_empty_declensions() {
        for flag in 0..=255 {
            let mut b = 0xabcdef01u32.to_le_bytes().to_vec();
            b.extend([255, 0]);
            b.extend(u32::MAX.to_le_bytes());
            b.push(flag);
            if flag != 0 {
                b.extend([0, 255, 0, 0, 0, 0]);
            }
            let expected = Response {
                pet_number: 0xabcdef01,
                name: vec![255],
                timestamp: u32::MAX,
                declined_flag: flag,
                declined_names: (flag != 0).then(|| [vec![], vec![255], vec![], vec![], vec![]]),
            };
            assert_eq!(decode(&b).unwrap(), expected);
            for n in 0..b.len() {
                assert!(decode(&b[..n]).is_err(), "flag{flag} prefix{n}");
            }
            b.push(0);
            assert!(decode(&b).is_err());
        }
    }
    #[test]
    fn empty_server_reply_remains_raw_not_a_shared_name_projection() {
        assert_eq!(
            decode(&[7, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
            Response {
                pet_number: 7,
                name: vec![],
                timestamp: 0,
                declined_flag: 0,
                declined_names: None,
            }
        );
    }
    #[test]
    fn query_is_number_then_full_guid_even_for_zero_and_unknown_bits() {
        for (number, guid) in [
            (0, 0),
            (u32::MAX, u64::MAX),
            (0x12345678, 0xf140112233445566),
        ] {
            let (op, b) = encode_query(number, guid);
            assert_eq!(op, 0x52);
            assert_eq!(&b[..4], &number.to_le_bytes());
            assert_eq!(&b[4..], &guid.to_le_bytes());
            assert_eq!(b.len(), 12);
        }
    }
}
