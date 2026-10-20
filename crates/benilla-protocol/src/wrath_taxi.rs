//! Build 12340 flight-master packets: AzerothCore TaxiHandler.cpp and
//! PlayerTaxi::AppendTaximaskTo (fourteen words, DBCStructure.h TaxiMaskSize).

use crate::events::SessionEvent;
use crate::types::TaxiMask;
use crate::wrath_session::{u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

pub const SMSG_SHOWTAXINODES: u16 = 0x1a9;
pub const SMSG_TAXINODE_STATUS: u16 = 0x1ab;
pub const SMSG_ACTIVATETAXIREPLY: u16 = 0x1ae;
pub const SMSG_NEW_TAXI_PATH: u16 = 0x1af;
pub const CMSG_TAXINODE_STATUS_QUERY: u32 = 0x1aa;
pub const CMSG_TAXIQUERYAVAILABLENODES: u32 = 0x1ac;
pub const CMSG_ACTIVATETAXI: u32 = 0x1ad;
pub const CMSG_ACTIVATETAXIEXPRESS: u32 = 0x312;

pub const fn handles(opcode: u16) -> bool {
    matches!(
        opcode,
        SMSG_SHOWTAXINODES | SMSG_TAXINODE_STATUS | SMSG_ACTIVATETAXIREPLY | SMSG_NEW_TAXI_PATH
    )
}

pub fn encode_query(guid: u64) -> Vec<u8> {
    guid.to_le_bytes().to_vec()
}

pub fn encode_activate(guid: u64, source: u32, destination: u32) -> Vec<u8> {
    let mut body = encode_query(guid);
    body.extend(source.to_le_bytes());
    body.extend(destination.to_le_bytes());
    body
}

pub fn encode_express(guid: u64, nodes: &[u32]) -> Vec<u8> {
    let mut body = encode_query(guid);
    // Unlike Vanilla, Wrath has no client-supplied total fare.
    body.extend((nodes.len() as u32).to_le_bytes());
    for node in nodes {
        body.extend(node.to_le_bytes());
    }
    body
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    let mut r = Cursor::new(body);
    let event = match opcode {
        SMSG_SHOWTAXINODES => {
            if u32_le(&mut r)? == 0 {
                None
            } else {
                let flightmaster = u64_le(&mut r)?;
                let nearest_node = u32_le(&mut r)?;
                let mut known_mask = TaxiMask::default();
                for word in &mut known_mask.0 {
                    *word = u32_le(&mut r)?;
                }
                Some(SessionEvent::TaxiNodesShown {
                    flightmaster,
                    nearest_node,
                    known_mask,
                })
            }
        }
        SMSG_TAXINODE_STATUS => Some(SessionEvent::TaxiNodeStatus {
            guid: u64_le(&mut r)?,
            known: u8_le(&mut r)? != 0,
        }),
        SMSG_ACTIVATETAXIREPLY => Some(SessionEvent::ActivateTaxiReply {
            code: u32_le(&mut r)?,
        }),
        SMSG_NEW_TAXI_PATH => Some(SessionEvent::NewTaxiPath),
        _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, "taxi opcode")),
    };
    if r.position() != body.len() as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "taxi trailing bytes",
        ));
    }
    Ok(event.into_iter().collect())
}

#[cfg(test)]
#[path = "wrath_taxi/tests.rs"]
mod tests;
