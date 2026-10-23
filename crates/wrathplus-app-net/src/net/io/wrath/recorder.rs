//! Dev-only decrypted inbound gameplay capture. Opt in with WOW_WRATH_PACKET_RECORD=<new path>.
//! Attached only to the post-auth/player-login reader, never to auth or the outbound writer.
//! Files may contain private character/chat data: keep local, never commit or share unreviewed.
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::Instant;

use wrathplus_protocol::wrath_world::WrathServerPacket;

// Version 1: magic (8), build u32 LE; records: elapsed_us u64 LE, SMSG opcode u16 LE,
// body_len u32 LE, decrypted body bytes. No wire cipher header, auth, or key material.
const HEADER: &[u8; 12] = b"AZBWRPK1\x34\x30\x00\x00"; // build 12340
const MAX_CAPTURE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_BODY_BYTES: usize = 0x7f_ffff - 2;

pub(super) struct Recorder {
    file: File,
    started: Instant,
    written: u64,
}

impl Recorder {
    pub(super) fn from_env() -> Option<Self> {
        let path = std::env::var_os("WOW_WRATH_PACKET_RECORD")?;
        if path.is_empty() {
            return None;
        }
        match Self::open(Path::new(&path)) {
            Ok(recorder) => Some(recorder),
            Err(error) => {
                bevy::log::warn!("Wrath packet recording disabled: {error}");
                None
            }
        }
    }

    fn open(path: &Path) -> io::Result<Self> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true); // never overwrite/append a previous session
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        file.write_all(HEADER)?;
        Ok(Self {
            file,
            started: Instant::now(),
            written: HEADER.len() as u64,
        })
    }

    fn record(&mut self, packet: &WrathServerPacket) -> io::Result<()> {
        // Defense in depth, even though none of these enters the gameplay reader.
        // AzerothCore Opcodes.h: AUTH_CHALLENGE/SESSION/RESPONSE, WARDEN_DATA.
        if matches!(packet.opcode, 0x1ec | 0x1ed | 0x1ee | 0x2e6) {
            return Ok(());
        }
        if packet.body.len() > MAX_BODY_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "oversized capture frame",
            ));
        }
        let size = 14 + packet.body.len() as u64;
        if self.written + size > MAX_CAPTURE_BYTES {
            return Err(io::Error::other("256 MiB capture limit reached"));
        }
        let elapsed = self.started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        self.file.write_all(&elapsed.to_le_bytes())?;
        self.file.write_all(&packet.opcode.to_le_bytes())?;
        self.file
            .write_all(&(packet.body.len() as u32).to_le_bytes())?;
        self.file.write_all(&packet.body)?;
        self.written += size;
        Ok(())
    }
}

/// Fail open: disable permanently after a write failure or cap; never interrupt gameplay.
pub(super) fn record(recorder: &mut Option<Recorder>, packet: &WrathServerPacket) {
    if let Some(active) = recorder.as_mut() {
        if let Err(error) = active.record(packet) {
            *recorder = None;
            bevy::log::warn!("Wrath packet recording stopped: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Capture(std::path::PathBuf);
    impl Capture {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!(
                "azb-recorder-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }
    }
    impl Drop for Capture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    fn packet(opcode: u16, body: &[u8]) -> WrathServerPacket {
        WrathServerPacket {
            opcode,
            body: body.to_vec(),
        }
    }

    #[test]
    fn captures_lossless_ordered_gameplay_frames_only() {
        let path = Capture::new();
        let mut recorder = Recorder::open(&path.0).unwrap();
        for opcode in [0x1ec, 0x1ed, 0x1ee, 0x2e6] {
            recorder
                .record(&packet(opcode, b"DO NOT RECORD AUTH"))
                .unwrap();
        }
        recorder.record(&packet(0xa9, &[0, 255, 1])).unwrap();
        recorder.record(&packet(0x496, &[])).unwrap();
        drop(recorder);
        let bytes = std::fs::read(&path.0).unwrap();
        assert_eq!(&bytes[..12], HEADER);
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 12340);
        assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 0xa9);
        assert_eq!(u32::from_le_bytes(bytes[22..26].try_into().unwrap()), 3);
        assert_eq!(&bytes[26..29], &[0, 255, 1]);
        assert_eq!(u16::from_le_bytes(bytes[37..39].try_into().unwrap()), 0x496);
        assert_eq!(&bytes[39..43], &[0; 4]);
        assert_eq!(bytes.len(), 43);
    }
    #[test]
    fn existing_capture_is_never_overwritten() {
        let path = Capture::new();
        std::fs::write(&path.0, b"keep").unwrap();
        assert_eq!(
            Recorder::open(&path.0).err().unwrap().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(std::fs::read(&path.0).unwrap(), b"keep");
    }
    #[test]
    fn limit_disables_capture_without_writing_partial_frame() {
        let path = Capture::new();
        let mut active = Recorder::open(&path.0).unwrap();
        active.written = MAX_CAPTURE_BYTES;
        let mut recorder = Some(active);
        record(&mut recorder, &packet(0xa9, b"body"));
        assert!(recorder.is_none());
        record(&mut recorder, &packet(0xa9, b"later"));
        assert_eq!(std::fs::read(&path.0).unwrap(), HEADER);
    }
    #[cfg(unix)]
    #[test]
    fn capture_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let path = Capture::new();
        let _recorder = Recorder::open(&path.0).unwrap();
        assert_eq!(
            std::fs::metadata(&path.0).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
