//! The bounded Wrath 3.3.5a world login and character-select protocol.
//!
//! This intentionally stops after bounded character creation / initial-login observation:
//! gameplay, movement, combat, and GM packets still use the vanilla path.
//! The layouts come from AzerothCore's `WorldSocket.cpp`, `ServerPktHeader.h`, and
//! `Player::BuildEnumData`.

use std::io::{Cursor, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use benilla_srp::wrath_header::{DecrypterHalf, EncrypterHalf, HeaderCrypto};
use benilla_srp::{NormalizedString, SESSION_KEY_LENGTH};
use flate2::write::ZlibEncoder;
use sha1::{Digest, Sha1};

/// The 3.3.5a client build accepted by AzerothCore.
pub const WRATH_CLIENT_BUILD: u32 = 12_340;
/// AzerothCore's usual world-server port.
pub const WRATH_WORLD_PORT: u16 = 8085;

const SMSG_AUTH_CHALLENGE: u16 = 0x1ec;
const CMSG_AUTH_SESSION: u32 = 0x1ed;
const SMSG_AUTH_RESPONSE: u16 = 0x1ee;
const SMSG_WARDEN_DATA: u16 = 0x2e6;
const CMSG_CHAR_ENUM: u32 = 0x037;
const SMSG_CHAR_ENUM: u16 = 0x03b;
const CMSG_CHAR_CREATE: u32 = 0x036;
const SMSG_CHAR_CREATE: u16 = 0x03a;
const CMSG_PLAYER_LOGIN: u32 = 0x03d;
const SMSG_CHARACTER_LOGIN_FAILED: u16 = 0x041;
const SMSG_LOGIN_SETTIMESPEED: u16 = 0x042;
const SMSG_TUTORIAL_FLAGS: u16 = 0x0fd;
const SMSG_ACCOUNT_DATA_TIMES: u16 = 0x209;
const SMSG_LOGIN_VERIFY_WORLD: u16 = 0x236;
const AUTH_OK: u8 = 0x0c;
const AUTH_WAIT_QUEUE: u8 = 0x1b;
const CHAR_CREATE_SUCCESS: u8 = 0x2f;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_DIAL_ENDPOINTS: usize = 16;
const IO_TIMEOUT: Duration = Duration::from_secs(10);
const QUEUE_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_STARTUP_PACKETS: usize = 64;
const MAX_QUEUE_UPDATES: usize = 64;
const MAX_ENUM_PACKETS: usize = 64;
const MAX_CREATE_PACKETS: usize = 64;
const MAX_INITIAL_LOGIN_PACKETS: usize = 512;
const MAX_CHARACTERS: usize = 50;
const MAX_SERVER_PACKET_SIZE: usize = 0x7f_ffff;

/// The world server requires Warden, which this initial Wrath protocol slice does not implement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrathWardenRequired;

impl std::fmt::Display for WrathWardenRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "this Wrath server requires Warden, which is not implemented"
        )
    }
}

impl std::error::Error for WrathWardenRequired {}

/// An AzerothCore world-authentication result other than `AUTH_OK` or `AUTH_WAIT_QUEUE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrathWorldAuthReject {
    pub code: u8,
}

impl std::fmt::Display for WrathWorldAuthReject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Wrath world server refused the session: result {:#04x}",
            self.code
        )
    }
}

impl std::error::Error for WrathWorldAuthReject {}

/// A normal 3.3.5a `CMSG_CHAR_CREATE` request.
///
/// Appearance values and the race/class combination remain server-authoritative. This layer only
/// rejects values that cannot name a playable Wrath race/class and the deliberately basic ASCII
/// name subset used by the probe API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrathCreateCharacter {
    pub name: String,
    pub race: u8,
    pub class: u8,
    pub gender: u8,
    pub skin: u8,
    pub face: u8,
    pub hair_style: u8,
    pub hair_color: u8,
    pub facial_hair: u8,
    /// The final byte read by AzerothCore. Normal character creation uses outfit zero.
    pub outfit_id: u8,
}

/// Local rejection of a malformed or out-of-range character-create request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrathCreateValidationError {
    reason: &'static str,
}

impl std::fmt::Display for WrathCreateValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid Wrath character creation request: {}",
            self.reason
        )
    }
}

impl std::error::Error for WrathCreateValidationError {}

/// A typed non-success result from `SMSG_CHAR_CREATE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrathCreateCharacterReject {
    pub code: u8,
}

impl std::fmt::Display for WrathCreateCharacterReject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Wrath server refused character creation: result {:#04x}",
            self.code
        )
    }
}

impl std::error::Error for WrathCreateCharacterReject {}

/// Metadata for one packet observed during the bounded initial-login exchange.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrathInitialPacket {
    pub opcode: u16,
    /// Payload bytes, excluding the server header and two-byte opcode.
    pub body_size: usize,
}

/// Packet evidence collected while entering the world.
///
/// The counters prove only that packets with these opcodes were received. In particular, this is
/// not an object model or evidence that the full world state was decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrathInitialLoginObservation {
    pub packets: Vec<WrathInitialPacket>,
    pub login_verify_world_count: usize,
    pub account_data_times_count: usize,
    pub tutorial_flags_count: usize,
    pub login_set_time_speed_count: usize,
}

/// A typed `SMSG_CHARACTER_LOGIN_FAILED` result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrathPlayerLoginReject {
    pub code: u8,
}

impl std::fmt::Display for WrathPlayerLoginReject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Wrath server refused player login: result {:#04x}",
            self.code
        )
    }
}

impl std::error::Error for WrathPlayerLoginReject {}

/// One of the 23 character-enumeration equipment slots, including the four bag slots.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WrathCharEnumItem {
    pub display_id: u32,
    pub inventory_type: u8,
    /// The visual spell aura chosen from the permanent or temporary enchantment.
    pub enchantment_aura: u32,
}

/// One Wrath `SMSG_CHAR_ENUM` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct WrathCharacter {
    pub guid: u64,
    pub name: String,
    pub race: u8,
    pub class: u8,
    pub gender: u8,
    pub skin: u8,
    pub face: u8,
    pub hair_style: u8,
    pub hair_color: u8,
    pub facial_hair: u8,
    pub level: u8,
    pub zone: u32,
    pub map: u32,
    pub position: [f32; 3],
    pub guild_id: u32,
    pub flags: u32,
    /// `CHAR_CUSTOMIZE_FLAG_*`, distinct from the character flags.
    pub customization_flags: u32,
    pub first_login: bool,
    pub pet_display_id: u32,
    pub pet_level: u32,
    pub pet_family: u32,
    pub equipment: [WrathCharEnumItem; 23],
}

#[derive(Debug)]
struct RawPacket {
    opcode: u16,
    body: Vec<u8>,
}

/// An authenticated Wrath world connection with bounded character-select and login observation.
pub struct WrathWorldSession {
    stream: TcpStream,
    crypto: HeaderCrypto,
}

impl WrathWorldSession {
    /// Connect, authenticate as `username`, and wait for admission to `realm_id`.
    pub fn connect(
        addr: impl ToSocketAddrs,
        username: &str,
        session_key: [u8; SESSION_KEY_LENGTH],
        realm_id: u32,
    ) -> Result<Self> {
        let username = NormalizedString::new(username)
            .map_err(|e| anyhow!("invalid username for Wrath world login: {e}"))?;
        // Reuse the SRP crate's OS-backed random u32 source; Wrath uses this as LocalChallenge.
        let client_seed = benilla_srp::vanilla_header::ProofSeed::new().seed();
        Self::connect_with_seed(addr, &username, session_key, realm_id, client_seed)
    }

    fn connect_with_seed(
        addr: impl ToSocketAddrs,
        username: &NormalizedString,
        session_key: [u8; SESSION_KEY_LENGTH],
        realm_id: u32,
        client_seed: u32,
    ) -> Result<Self> {
        let mut stream = connect_bounded(addr)?;
        stream
            .set_nodelay(true)
            .context("disabling Nagle on Wrath world socket")?;
        stream
            .set_read_timeout(Some(IO_TIMEOUT))
            .context("setting Wrath world read timeout")?;
        stream
            .set_write_timeout(Some(IO_TIMEOUT))
            .context("setting Wrath world write timeout")?;

        let challenge = recv_raw(&mut stream, None).context("reading SMSG_AUTH_CHALLENGE")?;
        if challenge.opcode != SMSG_AUTH_CHALLENGE {
            bail!(
                "expected SMSG_AUTH_CHALLENGE, got opcode {:#x}",
                challenge.opcode
            );
        }
        if challenge.body.len() != 40 {
            bail!(
                "SMSG_AUTH_CHALLENGE has {} bytes, expected 40",
                challenge.body.len()
            );
        }
        let server_seed = u32::from_le_bytes(challenge.body[4..8].try_into().unwrap());
        let proof = world_proof(username.as_ref(), client_seed, server_seed, &session_key);
        let body = auth_session_body(username.as_ref(), realm_id, client_seed, &proof)?;
        send_raw(&mut stream, None, CMSG_AUTH_SESSION, &body)
            .context("sending CMSG_AUTH_SESSION")?;

        let crypto = HeaderCrypto::from_session_key(session_key);
        let mut session = Self { stream, crypto };
        let mut queue_updates = 0usize;
        for _ in 0..MAX_STARTUP_PACKETS {
            let packet = session.recv_raw()?;
            match packet.opcode {
                SMSG_AUTH_RESPONSE => {
                    let result = *packet
                        .body
                        .first()
                        .ok_or_else(|| anyhow!("empty SMSG_AUTH_RESPONSE"))?;
                    match result {
                        AUTH_OK => {
                            if packet.body.len() != 11 {
                                bail!("AUTH_OK has {} bytes, expected 11", packet.body.len());
                            }
                            return Ok(session);
                        }
                        AUTH_WAIT_QUEUE => {
                            parse_queue_position(&packet.body)?;
                            queue_updates += 1;
                            if queue_updates > MAX_QUEUE_UPDATES {
                                bail!("Wrath login queue exceeded {MAX_QUEUE_UPDATES} updates");
                            }
                            session
                                .stream
                                .set_read_timeout(Some(QUEUE_TIMEOUT))
                                .context("setting bounded Wrath login-queue timeout")?;
                        }
                        code => return Err(WrathWorldAuthReject { code }.into()),
                    }
                }
                SMSG_WARDEN_DATA => return Err(WrathWardenRequired.into()),
                _ => {} // AzerothCore may interleave cache/account setup packets.
            }
        }
        bail!("no SMSG_AUTH_RESPONSE within {MAX_STARTUP_PACKETS} startup packets")
    }

    /// Request and parse one bounded Wrath character roster.
    pub fn enumerate_characters(&mut self) -> Result<Vec<WrathCharacter>> {
        self.stream
            .set_read_timeout(Some(IO_TIMEOUT))
            .context("restoring Wrath character-enumeration timeout")?;
        let encrypter = self.crypto.encrypter();
        send_raw(&mut self.stream, Some(encrypter), CMSG_CHAR_ENUM, &[])
            .context("sending CMSG_CHAR_ENUM")?;

        for _ in 0..MAX_ENUM_PACKETS {
            let packet = self.recv_raw()?;
            match packet.opcode {
                SMSG_CHAR_ENUM => return parse_character_enum(&packet.body),
                SMSG_WARDEN_DATA => return Err(WrathWardenRequired.into()),
                _ => {} // Only a bounded set of startup packets is ignored.
            }
        }
        bail!("no SMSG_CHAR_ENUM within {MAX_ENUM_PACKETS} packets")
    }

    /// Alias matching the existing vanilla session API.
    pub fn char_enum(&mut self) -> Result<Vec<WrathCharacter>> {
        self.enumerate_characters()
    }

    /// Validate and submit a normal character-create request, waiting for its one-byte result.
    pub fn create_character(&mut self, request: &WrathCreateCharacter) -> Result<()> {
        let body = create_character_body(request)?;
        let encrypter = self.crypto.encrypter();
        send_raw(&mut self.stream, Some(encrypter), CMSG_CHAR_CREATE, &body)
            .context("sending CMSG_CHAR_CREATE")?;

        for _ in 0..MAX_CREATE_PACKETS {
            let packet = self.recv_raw()?;
            match packet.opcode {
                SMSG_CHAR_CREATE => return parse_create_character_result(&packet.body),
                SMSG_WARDEN_DATA => return Err(WrathWardenRequired.into()),
                _ => {}
            }
        }
        bail!("no SMSG_CHAR_CREATE within {MAX_CREATE_PACKETS} packets")
    }

    /// Send `CMSG_PLAYER_LOGIN` and observe the bounded initial packet sequence.
    ///
    /// Collection ends at the first `SMSG_LOGIN_SETTIMESPEED` after
    /// `SMSG_LOGIN_VERIFY_WORLD`, the AzerothCore initial-packet marker. Bodies are intentionally
    /// not decoded as vanilla objects; only opcode and payload size are retained.
    pub fn observe_initial_login(&mut self, guid: u64) -> Result<WrathInitialLoginObservation> {
        let encrypter = self.crypto.encrypter();
        send_raw(
            &mut self.stream,
            Some(encrypter),
            CMSG_PLAYER_LOGIN,
            &guid.to_le_bytes(),
        )
        .context("sending CMSG_PLAYER_LOGIN")?;

        let mut observation = WrathInitialLoginObservation::new();
        for _ in 0..MAX_INITIAL_LOGIN_PACKETS {
            let packet = self.recv_raw()?;
            if packet.opcode == SMSG_WARDEN_DATA {
                return Err(WrathWardenRequired.into());
            }
            if packet.opcode == SMSG_CHARACTER_LOGIN_FAILED {
                let code = parse_single_result(&packet.body, "SMSG_CHARACTER_LOGIN_FAILED")?;
                return Err(WrathPlayerLoginReject { code }.into());
            }
            if observation.observe(&packet)? {
                return Ok(observation);
            }
        }
        bail!(
            "initial Wrath login did not reach SMSG_LOGIN_VERIFY_WORLD and subsequent \
             SMSG_LOGIN_SETTIMESPEED within {MAX_INITIAL_LOGIN_PACKETS} packets"
        )
    }

    fn recv_raw(&mut self) -> Result<RawPacket> {
        recv_raw(&mut self.stream, Some(self.crypto.decrypter()))
    }
}

impl WrathInitialLoginObservation {
    fn new() -> Self {
        Self {
            packets: Vec::new(),
            login_verify_world_count: 0,
            account_data_times_count: 0,
            tutorial_flags_count: 0,
            login_set_time_speed_count: 0,
        }
    }

    fn observe(&mut self, packet: &RawPacket) -> Result<bool> {
        match packet.opcode {
            SMSG_LOGIN_VERIFY_WORLD => {
                require_body_size(packet, 20, "SMSG_LOGIN_VERIFY_WORLD")?;
                self.login_verify_world_count += 1;
            }
            SMSG_ACCOUNT_DATA_TIMES => {
                validate_account_data_times(&packet.body)?;
                self.account_data_times_count += 1;
            }
            SMSG_TUTORIAL_FLAGS => {
                require_body_size(packet, 32, "SMSG_TUTORIAL_FLAGS")?;
                self.tutorial_flags_count += 1;
            }
            SMSG_LOGIN_SETTIMESPEED => {
                require_body_size(packet, 12, "SMSG_LOGIN_SETTIMESPEED")?;
                self.login_set_time_speed_count += 1;
            }
            _ => {}
        }
        self.packets.push(WrathInitialPacket {
            opcode: packet.opcode,
            body_size: packet.body.len(),
        });
        Ok(self.login_verify_world_count != 0 && self.login_set_time_speed_count != 0)
    }
}

fn create_character_body(request: &WrathCreateCharacter) -> Result<Vec<u8>> {
    let name = request.name.as_bytes();
    if !(2..=12).contains(&name.len()) {
        return Err(WrathCreateValidationError {
            reason: "name must contain 2 through 12 ASCII letters",
        }
        .into());
    }
    if !name.iter().all(u8::is_ascii_alphabetic) {
        return Err(WrathCreateValidationError {
            reason: "name must contain only ASCII letters",
        }
        .into());
    }
    if !matches!(request.race, 1..=8 | 10 | 11) {
        return Err(WrathCreateValidationError {
            reason: "race is not a playable 3.3.5 race",
        }
        .into());
    }
    if !matches!(request.class, 1..=9 | 11) {
        return Err(WrathCreateValidationError {
            reason: "class is not a playable 3.3.5 class",
        }
        .into());
    }
    if request.gender > 1 {
        return Err(WrathCreateValidationError {
            reason: "gender must be 0 or 1",
        }
        .into());
    }
    if request.outfit_id != 0 {
        return Err(WrathCreateValidationError {
            reason: "normal character creation requires outfit_id 0",
        }
        .into());
    }

    let mut body = Vec::with_capacity(name.len() + 10);
    body.extend_from_slice(name);
    body.push(0);
    body.extend_from_slice(&[
        request.race,
        request.class,
        request.gender,
        request.skin,
        request.face,
        request.hair_style,
        request.hair_color,
        request.facial_hair,
        request.outfit_id,
    ]);
    Ok(body)
}

fn parse_single_result(body: &[u8], packet_name: &str) -> Result<u8> {
    if body.len() != 1 {
        bail!("{packet_name} has {} bytes, expected 1", body.len());
    }
    Ok(body[0])
}

fn parse_create_character_result(body: &[u8]) -> Result<()> {
    let code = parse_single_result(body, "SMSG_CHAR_CREATE")?;
    if code == CHAR_CREATE_SUCCESS {
        Ok(())
    } else {
        Err(WrathCreateCharacterReject { code }.into())
    }
}

fn require_body_size(packet: &RawPacket, expected: usize, packet_name: &str) -> Result<()> {
    if packet.body.len() != expected {
        bail!(
            "{packet_name} has {} bytes, expected {expected}",
            packet.body.len()
        );
    }
    Ok(())
}

fn validate_account_data_times(body: &[u8]) -> Result<()> {
    if body.len() < 9 {
        bail!("truncated SMSG_ACCOUNT_DATA_TIMES: {} bytes", body.len());
    }
    let mask = u32::from_le_bytes(body[5..9].try_into().unwrap());
    let expected = 9 + mask.count_ones() as usize * 4;
    if body.len() != expected {
        bail!(
            "SMSG_ACCOUNT_DATA_TIMES has {} bytes, expected {expected} for mask {mask:#010x}",
            body.len()
        );
    }
    Ok(())
}

fn connect_bounded(addr: impl ToSocketAddrs) -> Result<TcpStream> {
    let addrs: Vec<_> = addr
        .to_socket_addrs()
        .context("resolving Wrath world address")?
        .take(MAX_DIAL_ENDPOINTS)
        .collect();
    if addrs.is_empty() {
        bail!("Wrath world address resolved to no endpoints");
    }
    let mut last = None;
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            Ok(stream) => return Ok(stream),
            Err(error) => last = Some((addr, error)),
        }
    }
    let (addr, error) = last.expect("non-empty address list");
    Err(error).with_context(|| format!("connecting to Wrath world server at {addr}"))
}

fn world_proof(
    account: &str,
    client_seed: u32,
    server_seed: u32,
    session_key: &[u8; SESSION_KEY_LENGTH],
) -> [u8; 20] {
    let mut sha = Sha1::new();
    sha.update(account.as_bytes());
    sha.update(0u32.to_le_bytes());
    sha.update(client_seed.to_le_bytes());
    sha.update(server_seed.to_le_bytes());
    sha.update(session_key);
    sha.finalize().into()
}

fn empty_addon_block() -> Result<Vec<u8>> {
    // Wrath's uncompressed form is addon count followed by current time. A zero-size tail is not
    // accepted by WorldSocket, while this is a valid list containing no stock/third-party addons.
    let plain = [0u8; 8];
    let mut encoder = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&plain)?;
    let compressed = encoder.finish()?;
    let mut out = Vec::with_capacity(4 + compressed.len());
    out.extend_from_slice(&(plain.len() as u32).to_le_bytes());
    out.extend_from_slice(&compressed);
    Ok(out)
}

fn auth_session_body(
    account: &str,
    realm_id: u32,
    client_seed: u32,
    proof: &[u8; 20],
) -> Result<Vec<u8>> {
    let addons = empty_addon_block()?;
    let mut body = Vec::with_capacity(64 + account.len() + addons.len());
    body.extend_from_slice(&WRATH_CLIENT_BUILD.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes()); // LoginServerID
    body.extend_from_slice(account.as_bytes());
    body.push(0);
    body.extend_from_slice(&0u32.to_le_bytes()); // LoginServerType
    body.extend_from_slice(&client_seed.to_le_bytes()); // LocalChallenge
    body.extend_from_slice(&0u32.to_le_bytes()); // RegionID
    body.extend_from_slice(&0u32.to_le_bytes()); // BattlegroupID
    body.extend_from_slice(&realm_id.to_le_bytes());
    body.extend_from_slice(&0u64.to_le_bytes()); // DosResponse
    body.extend_from_slice(proof);
    body.extend_from_slice(&addons);
    Ok(body)
}

fn send_raw(
    stream: &mut impl Write,
    encrypter: Option<&mut EncrypterHalf>,
    opcode: u32,
    body: &[u8],
) -> Result<()> {
    let size = body
        .len()
        .checked_add(4)
        .ok_or_else(|| anyhow!("client packet size overflow"))?;
    let size = u16::try_from(size)
        .map_err(|_| anyhow!("client packet body too large: {} bytes", body.len()))?;
    let size = size.to_be_bytes();
    let opcode = opcode.to_le_bytes();
    let mut header = [size[0], size[1], opcode[0], opcode[1], opcode[2], opcode[3]];
    if let Some(encrypter) = encrypter {
        encrypter.encrypt(&mut header);
    }
    stream.write_all(&header)?;
    stream.write_all(body)?;
    Ok(())
}

fn recv_raw(stream: &mut impl Read, decrypter: Option<&mut DecrypterHalf>) -> Result<RawPacket> {
    let mut first = [0u8; 4];
    stream
        .read_exact(&mut first)
        .context("reading Wrath server header")?;

    let (size, opcode) = if let Some(decrypter) = decrypter {
        // The fifth byte, when present, belongs to the same RC4 stream and must be consumed only
        // after the first four reveal the large-packet high bit.
        decrypter.decrypt(&mut first);
        decode_server_header(stream, first, Some(decrypter))?
    } else {
        decode_server_header(stream, first, None)?
    };
    if size < 2 {
        bail!("Wrath server header size {size} is smaller than its opcode");
    }
    if size > MAX_SERVER_PACKET_SIZE {
        bail!("Wrath server packet size {size} exceeds protocol limit");
    }
    let body_len = size - 2;
    let mut body = vec![0u8; body_len];
    stream
        .read_exact(&mut body)
        .with_context(|| format!("reading Wrath body for opcode {opcode:#x} ({body_len} bytes)"))?;
    Ok(RawPacket { opcode, body })
}

fn decode_server_header(
    stream: &mut impl Read,
    first: [u8; 4],
    decrypter: Option<&mut DecrypterHalf>,
) -> Result<(usize, u16)> {
    if first[0] & 0x80 == 0 {
        let size = usize::from(u16::from_be_bytes([first[0], first[1]]));
        return Ok((size, u16::from_le_bytes([first[2], first[3]])));
    }

    let mut last = [0u8; 1];
    stream
        .read_exact(&mut last)
        .context("reading fifth byte of large Wrath server header")?;
    if let Some(decrypter) = decrypter {
        decrypter.decrypt(&mut last);
    }
    let size =
        (usize::from(first[0] & 0x7f) << 16) | (usize::from(first[1]) << 8) | usize::from(first[2]);
    Ok((size, u16::from_le_bytes([first[3], last[0]])))
}

fn parse_queue_position(body: &[u8]) -> Result<u32> {
    // Full AUTH_WAIT_QUEUE: result, billing remaining/flags/rested/expansion, position, migration.
    if body.len() < 16 {
        bail!("truncated AUTH_WAIT_QUEUE: {} bytes", body.len());
    }
    Ok(u32::from_le_bytes(body[11..15].try_into().unwrap()))
}

fn parse_character_enum(body: &[u8]) -> Result<Vec<WrathCharacter>> {
    let (&count, entries) = body
        .split_first()
        .ok_or_else(|| anyhow!("empty SMSG_CHAR_ENUM"))?;
    let count = usize::from(count);
    if count > MAX_CHARACTERS {
        bail!("SMSG_CHAR_ENUM count {count} exceeds {MAX_CHARACTERS}");
    }
    let mut cursor = Cursor::new(entries);
    let mut characters = Vec::with_capacity(count);
    for index in 0..count {
        characters.push(
            read_character(&mut cursor)
                .with_context(|| format!("parsing Wrath character {index}"))?,
        );
    }
    if cursor.position() != entries.len() as u64 {
        bail!(
            "SMSG_CHAR_ENUM has {} trailing bytes",
            entries.len() as u64 - cursor.position()
        );
    }
    Ok(characters)
}

fn read_character(r: &mut Cursor<&[u8]>) -> Result<WrathCharacter> {
    let guid = read_u64(r)?;
    let name = read_cstring(r, 63)?;
    let race = read_u8(r)?;
    let class = read_u8(r)?;
    let gender = read_u8(r)?;
    let skin = read_u8(r)?;
    let face = read_u8(r)?;
    let hair_style = read_u8(r)?;
    let hair_color = read_u8(r)?;
    let facial_hair = read_u8(r)?;
    let level = read_u8(r)?;
    let zone = read_u32(r)?;
    let map = read_u32(r)?;
    let position = [read_f32(r)?, read_f32(r)?, read_f32(r)?];
    let guild_id = read_u32(r)?;
    let flags = read_u32(r)?;
    let customization_flags = read_u32(r)?;
    let first_login = read_u8(r)? != 0;
    let pet_display_id = read_u32(r)?;
    let pet_level = read_u32(r)?;
    let pet_family = read_u32(r)?;
    let mut equipment = [WrathCharEnumItem::default(); 23];
    for item in &mut equipment {
        item.display_id = read_u32(r)?;
        item.inventory_type = read_u8(r)?;
        item.enchantment_aura = read_u32(r)?;
    }
    Ok(WrathCharacter {
        guid,
        name,
        race,
        class,
        gender,
        skin,
        face,
        hair_style,
        hair_color,
        facial_hair,
        level,
        zone,
        map,
        position,
        guild_id,
        flags,
        customization_flags,
        first_login,
        pet_display_id,
        pet_level,
        pet_family,
        equipment,
    })
}

fn read_exact<const N: usize>(r: &mut impl Read) -> Result<[u8; N]> {
    let mut bytes = [0u8; N];
    r.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn read_u8(r: &mut impl Read) -> Result<u8> {
    Ok(read_exact::<1>(r)?[0])
}

fn read_u32(r: &mut impl Read) -> Result<u32> {
    Ok(u32::from_le_bytes(read_exact(r)?))
}

fn read_u64(r: &mut impl Read) -> Result<u64> {
    Ok(u64::from_le_bytes(read_exact(r)?))
}

fn read_f32(r: &mut impl Read) -> Result<f32> {
    Ok(f32::from_le_bytes(read_exact(r)?))
}

fn read_cstring(r: &mut impl Read, max_len: usize) -> Result<String> {
    let mut bytes = Vec::new();
    loop {
        let byte = read_u8(r).context("truncated character name")?;
        if byte == 0 {
            return String::from_utf8(bytes).context("character name is not UTF-8");
        }
        if bytes.len() == max_len {
            bail!("character name exceeds {max_len} bytes");
        }
        bytes.push(byte);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::ZlibDecoder;

    fn enum_fixture() -> Vec<u8> {
        let mut body = vec![1];
        body.extend_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes());
        body.extend_from_slice(b"Arthas\0");
        body.extend_from_slice(&[1, 6, 0, 2, 3, 4, 5, 6, 80]);
        body.extend_from_slice(&12u32.to_le_bytes());
        body.extend_from_slice(&571u32.to_le_bytes());
        for value in [1.5f32, -2.25, 3.75] {
            body.extend_from_slice(&value.to_le_bytes());
        }
        body.extend_from_slice(&44u32.to_le_bytes());
        body.extend_from_slice(&0x0400u32.to_le_bytes());
        body.extend_from_slice(&2u32.to_le_bytes());
        body.push(1);
        body.extend_from_slice(&123u32.to_le_bytes());
        body.extend_from_slice(&80u32.to_le_bytes());
        body.extend_from_slice(&15u32.to_le_bytes());
        for slot in 0..23u32 {
            body.extend_from_slice(&(1000 + slot).to_le_bytes());
            body.push(slot as u8);
            body.extend_from_slice(&(2000 + slot).to_le_bytes());
        }
        body
    }

    #[test]
    fn proof_matches_fixed_sha1_vector() {
        let key = [0x5au8; SESSION_KEY_LENGTH];
        assert_eq!(
            world_proof("ACCOUNT", 0x1122_3344, 0xaabb_ccdd, &key),
            [
                0x08, 0x08, 0x5a, 0x52, 0xa9, 0xbf, 0xb4, 0x83, 0x3f, 0x9a, 0x74, 0x23, 0x03, 0x39,
                0x18, 0xc0, 0xbc, 0x78, 0x43, 0xf9,
            ]
        );
    }

    #[test]
    fn auth_session_has_exact_fixed_fields_and_valid_empty_addons() {
        let proof = [0xabu8; 20];
        let body = auth_session_body("ACCOUNT", 7, 0x1122_3344, &proof).unwrap();
        assert_eq!(&body[0..4], &WRATH_CLIENT_BUILD.to_le_bytes());
        assert_eq!(&body[4..8], &[0; 4]);
        assert_eq!(&body[8..16], b"ACCOUNT\0");
        assert_eq!(&body[16..20], &[0; 4]);
        assert_eq!(&body[20..24], &0x1122_3344u32.to_le_bytes());
        assert_eq!(&body[24..32], &[0; 8]);
        assert_eq!(&body[32..36], &7u32.to_le_bytes());
        assert_eq!(&body[36..44], &[0; 8]);
        assert_eq!(&body[44..64], &proof);
        let plain_len = u32::from_le_bytes(body[64..68].try_into().unwrap());
        assert_eq!(plain_len, 8);
        let mut plain = Vec::new();
        ZlibDecoder::new(&body[68..])
            .read_to_end(&mut plain)
            .unwrap();
        assert_eq!(plain, [0; 8]);
    }

    #[test]
    fn server_headers_support_small_and_large_sizes() {
        let mut small = Cursor::new(Vec::<u8>::new());
        assert_eq!(
            decode_server_header(&mut small, [0x01, 0x02, 0x34, 0x12], None).unwrap(),
            (0x102, 0x1234)
        );
        let mut fifth = Cursor::new(vec![0x12]);
        assert_eq!(
            decode_server_header(&mut fifth, [0x81, 0x23, 0x45, 0x34], None).unwrap(),
            (0x01_2345, 0x1234)
        );
        assert!(
            decode_server_header(&mut Cursor::new(Vec::<u8>::new()), [0x80, 0, 2, 0x34], None)
                .is_err()
        );
    }

    #[test]
    fn client_header_rejects_u16_size_overflow() {
        let error = send_raw(
            &mut Vec::new(),
            None,
            CMSG_CHAR_ENUM,
            &vec![0; usize::from(u16::MAX) - 3],
        )
        .unwrap_err();
        assert!(error.to_string().contains("too large"));
    }

    #[test]
    fn character_fixture_covers_wrath_only_fields_and_23_slots() {
        let characters = parse_character_enum(&enum_fixture()).unwrap();
        let character = &characters[0];
        assert_eq!(character.name, "Arthas");
        assert_eq!(character.customization_flags, 2);
        assert!(character.first_login);
        assert_eq!(character.pet_display_id, 123);
        assert_eq!(character.equipment.len(), 23);
        assert_eq!(character.equipment[22].display_id, 1022);
        assert_eq!(character.equipment[22].enchantment_aura, 2022);
    }

    #[test]
    fn malformed_or_truncated_character_enums_fail() {
        assert!(parse_character_enum(&[]).is_err());
        assert!(parse_character_enum(&[51]).is_err());
        let fixture = enum_fixture();
        for cut in [1, 12, fixture.len() - 1] {
            assert!(parse_character_enum(&fixture[..cut]).is_err(), "cut {cut}");
        }
        let mut trailing = fixture;
        trailing.push(0xff);
        assert!(parse_character_enum(&trailing).is_err());
    }

    #[test]
    fn malformed_packet_sizes_and_truncated_bodies_fail() {
        assert!(recv_raw(&mut Cursor::new(vec![0, 1, 0x3b, 0]), None).is_err());
        assert!(recv_raw(&mut Cursor::new(vec![0, 5, 0x3b, 0, 1]), None).is_err());
    }

    fn create_fixture() -> WrathCreateCharacter {
        WrathCreateCharacter {
            name: "Arthas".into(),
            race: 1,
            class: 2,
            gender: 0,
            skin: 3,
            face: 4,
            hair_style: 5,
            hair_color: 6,
            facial_hair: 7,
            outfit_id: 0,
        }
    }

    #[test]
    fn character_create_has_exact_wrath_wire_order_and_client_header() {
        let body = create_character_body(&create_fixture()).unwrap();
        assert_eq!(body, b"Arthas\0\x01\x02\x00\x03\x04\x05\x06\x07\x00");

        let mut wire = Vec::new();
        send_raw(&mut wire, None, CMSG_CHAR_CREATE, &body).unwrap();
        assert_eq!(&wire[..6], &[0, 0x14, 0x36, 0, 0, 0]);
        assert_eq!(&wire[6..], body);
    }

    #[test]
    fn character_create_validation_rejects_bad_names_ids_gender_and_outfit() {
        for name in ["A", "ThirteenChars", "Bad-name", "Åke"] {
            let mut request = create_fixture();
            request.name = name.into();
            let error = create_character_body(&request).unwrap_err();
            assert!(error.downcast_ref::<WrathCreateValidationError>().is_some());
        }
        for (field, value) in [("race", 9), ("class", 10), ("gender", 2), ("outfit", 1)] {
            let mut request = create_fixture();
            match field {
                "race" => request.race = value,
                "class" => request.class = value,
                "gender" => request.gender = value,
                "outfit" => request.outfit_id = value,
                _ => unreachable!(),
            }
            assert!(
                create_character_body(&request).is_err(),
                "accepted {field}={value}"
            );
        }
    }

    #[test]
    fn character_create_result_is_typed_and_rejects_truncation() {
        assert!(parse_create_character_result(&[CHAR_CREATE_SUCCESS]).is_ok());
        let error = parse_create_character_result(&[0x32]).unwrap_err();
        assert_eq!(
            error.downcast_ref::<WrathCreateCharacterReject>(),
            Some(&WrathCreateCharacterReject { code: 0x32 })
        );
        assert!(parse_create_character_result(&[]).is_err());
        assert!(parse_create_character_result(&[CHAR_CREATE_SUCCESS, 0]).is_err());
    }

    #[test]
    fn player_login_is_full_little_endian_guid_packet() {
        let guid = 0x0102_0304_0506_0708u64;
        let mut wire = Vec::new();
        send_raw(&mut wire, None, CMSG_PLAYER_LOGIN, &guid.to_le_bytes()).unwrap();
        assert_eq!(&wire[..6], &[0, 12, 0x3d, 0, 0, 0]);
        assert_eq!(&wire[6..], &guid.to_le_bytes());
    }

    #[test]
    fn initial_login_observation_counts_evidence_without_parsing_objects() {
        let mut observation = WrathInitialLoginObservation::new();
        assert!(!observation
            .observe(&RawPacket {
                opcode: SMSG_TUTORIAL_FLAGS,
                body: vec![0; 32],
            })
            .unwrap());
        let mut account_data = vec![0; 9 + 3 * 4];
        account_data[5..9].copy_from_slice(&0b1000_0011u32.to_le_bytes());
        assert!(!observation
            .observe(&RawPacket {
                opcode: SMSG_ACCOUNT_DATA_TIMES,
                body: account_data,
            })
            .unwrap());
        assert!(!observation
            .observe(&RawPacket {
                opcode: SMSG_LOGIN_VERIFY_WORLD,
                body: vec![0; 20],
            })
            .unwrap());
        assert!(observation
            .observe(&RawPacket {
                opcode: SMSG_LOGIN_SETTIMESPEED,
                body: vec![0; 12],
            })
            .unwrap());

        assert_eq!(observation.login_verify_world_count, 1);
        assert_eq!(observation.account_data_times_count, 1);
        assert_eq!(observation.tutorial_flags_count, 1);
        assert_eq!(observation.login_set_time_speed_count, 1);
        assert_eq!(observation.packets.len(), 4);
        assert_eq!(observation.packets[2].body_size, 20);
    }

    #[test]
    fn initial_login_known_packets_reject_truncated_or_inconsistent_bodies() {
        for (opcode, size) in [
            (SMSG_LOGIN_VERIFY_WORLD, 19),
            (SMSG_TUTORIAL_FLAGS, 31),
            (SMSG_LOGIN_SETTIMESPEED, 11),
        ] {
            assert!(WrathInitialLoginObservation::new()
                .observe(&RawPacket {
                    opcode,
                    body: vec![0; size],
                })
                .is_err());
        }
        assert!(validate_account_data_times(&[0; 8]).is_err());
        let mut inconsistent = vec![0; 9];
        inconsistent[5..9].copy_from_slice(&1u32.to_le_bytes());
        assert!(validate_account_data_times(&inconsistent).is_err());
    }

    #[test]
    fn queue_response_layout_is_bounded_and_complete() {
        assert!(parse_queue_position(&[0; 15]).is_err());
        let mut body = vec![0; 16];
        body[0] = AUTH_WAIT_QUEUE;
        body[11..15].copy_from_slice(&1234u32.to_le_bytes());
        assert_eq!(parse_queue_position(&body).unwrap(), 1234);
    }
}
