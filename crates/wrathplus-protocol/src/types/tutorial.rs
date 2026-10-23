//! Tutorial wire: one inbound bank of bits and the client's three tutorial sends. The 1.12
//! client sizes its banks from every remaining byte of the packet; vmangos sends 32 bytes.

/// `SMSG_TUTORIAL_FLAGS` (handler `0x4b5700`): bit `id` is `bytes[id >> 3] & (1 << (id & 7))`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TutorialFlags {
    pub bytes: Vec<u8>,
}
