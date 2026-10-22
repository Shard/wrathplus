//! The hasher for handle-keyed maps.

use std::collections::HashMap;

/// A map keyed by handles, hashed by [`HandleHasher`]: the layout's full derive looks up every
/// live frame and region several times, and SipHash there was a third of the stock load.
pub(crate) type HandleMap<K, V> = HashMap<K, V, std::hash::BuildHasherDefault<HandleHasher>>;

/// The Fx multiply-rotate hash (rustc's), for small trusted keys: a handle's two `u32`s. Not
/// DoS-resistant, which a handle needs no more than an arena index does.
#[derive(Default, Clone, Copy)]
pub(crate) struct HandleHasher(u64);

impl HandleHasher {
    const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

    #[inline]
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(Self::SEED);
    }
}

impl std::hash::Hasher for HandleHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.add(u64::from_le_bytes(word));
        }
    }

    #[inline]
    fn write_u32(&mut self, n: u32) {
        self.add(u64::from(n));
    }

    #[inline]
    fn write_u64(&mut self, n: u64) {
        self.add(n);
    }

    #[inline]
    fn finish(&self) -> u64 {
        // hashbrown takes its control bits from the top: fold the well-mixed high half down too.
        self.0.rotate_left(26)
    }
}
