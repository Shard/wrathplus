//! The installed ambience beds through the streaming decoder.

use super::{sfx_from_bytes, stream_from_source};

/// Every bed in the 3.3.5 install streams at its data chunk's length. None of them stores the
/// mono `nBlockAlign` that 1.12's did; `a_stereo_bed_streams_its_true_length` in `mixer` keeps
/// that regression.
#[test]
fn real_beds_stream_their_data_chunks_frames() {
    use std::io::Read;
    let data = wrathplus_formats::wow_data_or_skip!();
    let Ok(chain) = wrathplus_formats::open_chain(&data) else {
        eprintln!("skipping: no client data at {}", data.display());
        return;
    };
    let beds: Vec<String> = chain
        .list()
        .expect("listing")
        .into_iter()
        .map(|e| e.name)
        .filter(|n| {
            let n = n.to_ascii_lowercase();
            n.starts_with("sound\\ambience\\") && n.ends_with(".wav")
        })
        .collect();
    assert!(beds.len() > 100, "the install's beds: {}", beds.len());
    for path in &beds {
        let archive = chain.archive_for(path).expect("in the chain");
        let mut head = vec![0u8; 4096];
        let n = archive.open_file(path).unwrap().read(&mut head).unwrap();
        head.truncate(n);
        let (frame, data_len) = wav_frame_and_data_len(&head).expect(path);

        let file = archive.open_file(path).expect("opens");
        let len = file.size();
        let streamed = stream_from_source(file, len).expect(path).num_frames();
        assert_eq!(streamed, data_len / frame, "{path}: streamed frames");
    }

    let forest = "Sound\\Ambience\\ZoneAmbience\\ForestNormalDay.wav";
    let decoded = sfx_from_bytes(chain.read(forest).unwrap()).unwrap();
    assert_eq!(decoded.num_frames(), 60 * 22_050, "decoded whole, 60 s");
}

/// A PCM WAV header's frame size (channels × bytes per sample) and data chunk length.
fn wav_frame_and_data_len(head: &[u8]) -> Option<(usize, usize)> {
    let u16_at = |at: usize| Some(u16::from_le_bytes(head.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_le_bytes(head.get(at..at + 4)?.try_into().ok()?));
    let (mut at, mut fmt) = (12usize, None);
    loop {
        let (id, len) = (head.get(at..at + 4)?, u32_at(at + 4)? as usize);
        if id == b"fmt " {
            let frame = usize::from(u16_at(at + 10)?) * usize::from(u16_at(at + 22)? / 8);
            fmt = Some(frame);
        } else if id == b"data" {
            return Some((fmt?, len));
        }
        at += 8 + len + (len & 1);
    }
}
