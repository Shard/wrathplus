//! `GetRandomName` (`0x4dfca0`), build 12340's letter-window walk over the selected race and sex's
//! `NameGen.dbc` names. The builder (`0x4e6790` into `0x4e6560`) formats each name with
//! `0x9f67cc` (inferred `"_%s"`: the walk starts on a `_` window), lowercases it and counts every
//! window of three letters, the last one two (an end mark), in first-seen order. The walk
//! (`0x4e63a0`) starts on a weighted `_` window and appends the third letter of a weighted window
//! whose first two match the name's last two, until an end mark or the length cap, then drops the
//! `_` and capitalizes. `GetRandomName` rolls until the name check passes with 4 to 10 letters.
//!
//! Labelled: the random stream (`0x464580` over `0xb2fa30`, scaled by `__aullshr`) is not the
//! reference's, so the same seed names differently; the name check (`0x6b0f90` into `0x7e1e90`)
//! is approximated as letters only with no letter three times running.

/// One counted window: up to three lowercase letters and its count.
pub(super) type Window = (Vec<u8>, u32);

/// `0x4e6560`: every name's windows, counted in first-seen order.
pub(super) fn windows(names: &[String]) -> Vec<Window> {
    let mut table: Vec<Window> = Vec::new();
    for name in names {
        let s: Vec<u8> = format!("_{name}").to_ascii_lowercase().into_bytes();
        // `strlen - 1` windows of up to three letters (a four-byte copy with its terminator).
        for i in 0..s.len().saturating_sub(1) {
            let key = s[i..(i + 3).min(s.len())].to_vec();
            match table.iter_mut().find(|(k, _)| *k == key) {
                Some((_, n)) => *n += 1,
                None => table.push((key, 1)),
            }
        }
    }
    table
}

/// The run of windows `0x4e63a0` picks among: the indices from the first match to the last of a
/// run, as it scans (a lone match does not end the scan, a run of two or more does).
fn candidates(table: &[Window], matches: impl Fn(&[u8]) -> bool) -> Option<(usize, usize)> {
    let (mut lo, mut hi) = (table.len(), 0usize);
    for (i, (key, _)) in table.iter().enumerate() {
        if matches(key) {
            lo = lo.min(i);
            hi = hi.max(i);
        } else if lo < hi {
            break;
        }
    }
    (lo <= hi && lo < table.len()).then_some((lo, hi))
}

/// `0x4e63a0`: one walk into at most `cap - 1` bytes.
pub(super) fn generate(table: &[Window], cap: usize, mut roll: impl FnMut() -> u64) -> String {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let range = if buf.is_empty() {
            candidates(table, |k| k.first() == Some(&b'_'))
        } else {
            let tail = buf[buf.len().saturating_sub(2)..].to_vec();
            candidates(table, move |k| k.len() >= 2 && k[..2] == tail[..])
        };
        let Some((lo, hi)) = range else { break };
        let total: u64 = table[lo..=hi].iter().map(|(_, n)| u64::from(*n)).sum();
        let mut r = roll() % total.max(1);
        let mut pick = lo;
        for (i, (_, n)) in table.iter().enumerate().take(hi + 1).skip(lo) {
            pick = i;
            if r < u64::from(*n) {
                break;
            }
            r -= u64::from(*n);
        }
        let key = &table[pick].0;
        if buf.is_empty() {
            buf.extend_from_slice(key);
            continue;
        }
        // An end mark (no third letter) ends the name.
        let Some(&next) = key.get(2) else { break };
        buf.push(next);
        if buf.len() >= cap.saturating_sub(1) {
            break;
        }
    }
    if !buf.is_empty() {
        buf.remove(0);
    }
    if let Some(first) = buf.first_mut() {
        first.make_ascii_uppercase();
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// The approximated name check: letters only, never one letter three times running.
pub(super) fn plausible(name: &str) -> bool {
    let b = name.as_bytes();
    b.iter().all(u8::is_ascii_alphabetic)
        && !b
            .windows(3)
            .any(|w| w[0].eq_ignore_ascii_case(&w[1]) && w[1].eq_ignore_ascii_case(&w[2]))
}

/// `GetRandomName`: walk until the check passes with 4 to 10 letters, or a walk comes back empty
/// (no names for the race and sex), as `0x4dfca0`'s loop does; bounded here.
pub(super) fn random_name(names: &[String], mut roll: impl FnMut() -> u64) -> String {
    let table = windows(names);
    for _ in 0..1000 {
        let name = generate(&table, 0xe, &mut roll);
        if name.is_empty() || (plausible(&name) && (4..=10).contains(&name.len())) {
            return name;
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_count_threes_and_end_on_a_two() {
        let t = windows(&["Abc".into(), "Abd".into()]);
        let keys: Vec<(String, u32)> = t
            .iter()
            .map(|(k, n)| (String::from_utf8_lossy(k).into_owned(), *n))
            .collect();
        assert_eq!(
            keys,
            vec![
                ("_ab".into(), 2),
                ("abc".into(), 1),
                ("bc".into(), 1),
                ("abd".into(), 1),
                ("bd".into(), 1)
            ]
        );
    }

    #[test]
    fn a_walk_over_one_name_rebuilds_it_and_rolls_stay_plausible() {
        let mut seed = 1u64;
        let mut roll = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            seed >> 33
        };
        assert_eq!(random_name(&["Thrall".into()], &mut roll), "Thrall");
        let names: Vec<String> = ["Agamand", "Andromath", "Antonidas", "Fordred", "Nielas"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        for _ in 0..50 {
            let n = random_name(&names, &mut roll);
            assert!((4..=10).contains(&n.len()) && plausible(&n), "{n}");
            assert!(n.chars().next().unwrap().is_ascii_uppercase(), "{n}");
        }
        assert_eq!(random_name(&[], &mut roll), "");
    }
}
