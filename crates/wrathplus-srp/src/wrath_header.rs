//! Wrath 3.3.5a world-header encryption.
//!
//! After `CMSG_AUTH_SESSION`, each connection direction uses RC4-drop1024 with a distinct
//! HMAC-SHA1-derived key. The client encrypts with AzerothCore's server-decryption seed and
//! decrypts with its server-encryption seed. This module only advances a byte stream; packet
//! header framing, including Wrath's variable-width server size field, belongs to the caller.

use sha1::{Digest, Sha1};

use crate::SESSION_KEY_LENGTH;

const CLIENT_ENCRYPTION_SEED: [u8; 16] = [
    0xc2, 0xb3, 0x72, 0x3c, 0xc6, 0xae, 0xd9, 0xb5, 0x34, 0x3c, 0x53, 0xee, 0x2f, 0x43, 0x67, 0xce,
];
const SERVER_ENCRYPTION_SEED: [u8; 16] = [
    0xcc, 0x98, 0xae, 0x04, 0xe8, 0x97, 0xea, 0xca, 0x12, 0xdd, 0xc0, 0x93, 0x42, 0x91, 0x53, 0x57,
];
const RC4_DROP_BYTES: usize = 1024;

fn hmac_sha1(key: &[u8], message: &[u8]) -> [u8; 20] {
    const BLOCK_LENGTH: usize = 64;

    let mut padded_key = [0u8; BLOCK_LENGTH];
    if key.len() > BLOCK_LENGTH {
        let digest: [u8; 20] = Sha1::digest(key).into();
        padded_key[..digest.len()].copy_from_slice(&digest);
    } else {
        padded_key[..key.len()].copy_from_slice(key);
    }

    let mut inner_pad = [0x36; BLOCK_LENGTH];
    let mut outer_pad = [0x5c; BLOCK_LENGTH];
    for ((inner, outer), key_byte) in inner_pad
        .iter_mut()
        .zip(outer_pad.iter_mut())
        .zip(padded_key)
    {
        *inner ^= key_byte;
        *outer ^= key_byte;
    }

    let mut inner = Sha1::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner_digest = inner.finalize();

    let mut outer = Sha1::new();
    outer.update(outer_pad);
    outer.update(inner_digest);
    outer.finalize().into()
}

#[derive(Debug, Clone)]
struct Rc4 {
    state: [u8; 256],
    i: u8,
    j: u8,
}

impl Rc4 {
    fn new(key: &[u8]) -> Self {
        debug_assert!(!key.is_empty());

        let mut state = [0u8; 256];
        for (value, byte) in (0u8..=u8::MAX).zip(state.iter_mut()) {
            *byte = value;
        }

        let mut j = 0u8;
        for i in 0..state.len() {
            j = j.wrapping_add(state[i]).wrapping_add(key[i % key.len()]);
            state.swap(i, j as usize);
        }

        let mut cipher = Self { state, i: 0, j: 0 };
        let mut discarded = [0u8; RC4_DROP_BYTES];
        cipher.apply(&mut discarded);
        cipher
    }

    fn apply(&mut self, data: &mut [u8]) {
        for byte in data {
            self.i = self.i.wrapping_add(1);
            self.j = self.j.wrapping_add(self.state[self.i as usize]);
            self.state.swap(self.i as usize, self.j as usize);
            let index = self.state[self.i as usize].wrapping_add(self.state[self.j as usize]);
            *byte ^= self.state[index as usize];
        }
    }
}

/// The client-to-server half of the Wrath header cipher.
#[derive(Debug, Clone)]
pub struct EncrypterHalf {
    cipher: Rc4,
}

impl EncrypterHalf {
    /// Encrypt bytes in place, advancing the client-to-server stream.
    pub fn encrypt(&mut self, data: &mut [u8]) {
        self.cipher.apply(data);
    }
}

/// The server-to-client half of the Wrath header cipher.
#[derive(Debug, Clone)]
pub struct DecrypterHalf {
    cipher: Rc4,
}

impl DecrypterHalf {
    /// Decrypt any contiguous header bytes in place, advancing the server-to-client stream.
    ///
    /// Calls may split a header at arbitrary byte boundaries, but must preserve wire order.
    pub fn decrypt(&mut self, data: &mut [u8]) {
        self.cipher.apply(data);
    }
}

/// Both client-side directions of the Wrath header cipher.
#[derive(Debug, Clone)]
pub struct HeaderCrypto {
    encrypt: EncrypterHalf,
    decrypt: DecrypterHalf,
}

impl HeaderCrypto {
    /// Construct client-side header crypto from the 40-byte SRP session key.
    pub fn from_session_key(session_key: [u8; SESSION_KEY_LENGTH]) -> Self {
        let client_key = hmac_sha1(&CLIENT_ENCRYPTION_SEED, &session_key);
        let server_key = hmac_sha1(&SERVER_ENCRYPTION_SEED, &session_key);
        Self {
            encrypt: EncrypterHalf {
                cipher: Rc4::new(&client_key),
            },
            decrypt: DecrypterHalf {
                cipher: Rc4::new(&server_key),
            },
        }
    }

    pub fn decrypter(&mut self) -> &mut DecrypterHalf {
        &mut self.decrypt
    }

    pub fn encrypter(&mut self) -> &mut EncrypterHalf {
        &mut self.encrypt
    }

    /// Split into halves, one per direction of a split socket.
    pub fn split(self) -> (EncrypterHalf, DecrypterHalf) {
        (self.encrypt, self.decrypt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION_KEY: [u8; SESSION_KEY_LENGTH] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
        0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27,
    ];
    const PLAINTEXT: [u8; 32] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
        0x1e, 0x1f,
    ];
    const CLIENT_CIPHERTEXT: [u8; 32] = [
        0xe6, 0x56, 0x8a, 0xe5, 0xa2, 0xcb, 0x41, 0x82, 0x58, 0xdf, 0x64, 0x8c, 0xb9, 0xce, 0x10,
        0xd1, 0x5a, 0x46, 0x25, 0xb2, 0x36, 0xec, 0xbb, 0x0d, 0x7e, 0xc4, 0x3a, 0x32, 0x46, 0x8f,
        0xda, 0x8d,
    ];
    const SERVER_CIPHERTEXT: [u8; 32] = [
        0xda, 0x46, 0x72, 0xc7, 0xeb, 0xd5, 0xba, 0xec, 0x79, 0x7a, 0xc1, 0x1f, 0x5d, 0x64, 0xef,
        0x32, 0x13, 0xb6, 0xd3, 0xc2, 0xdc, 0x94, 0x26, 0x11, 0x64, 0x47, 0x46, 0x8e, 0xe9, 0x09,
        0x9a, 0xdf,
    ];

    #[test]
    fn openssl_known_answers_pin_key_derivation_and_drop1024() {
        // Independently generated with OpenSSL HMAC-SHA1 and legacy-provider RC4.
        assert_eq!(
            hmac_sha1(&CLIENT_ENCRYPTION_SEED, &SESSION_KEY),
            [
                0x71, 0x9a, 0x8b, 0xf1, 0x29, 0xdb, 0xc2, 0x92, 0x44, 0x79, 0xb6, 0x6c, 0x84, 0x17,
                0x3a, 0x29, 0xef, 0x9b, 0xd3, 0x43,
            ]
        );
        assert_eq!(
            hmac_sha1(&SERVER_ENCRYPTION_SEED, &SESSION_KEY),
            [
                0x31, 0x6e, 0xcb, 0x67, 0x5e, 0x32, 0x6f, 0x11, 0xc2, 0xf4, 0x70, 0x35, 0xf7, 0x56,
                0xa7, 0x92, 0x63, 0x81, 0x7b, 0x4a,
            ]
        );

        let (mut encrypt, _) = HeaderCrypto::from_session_key(SESSION_KEY).split();
        let mut encrypted = PLAINTEXT;
        encrypt.encrypt(&mut encrypted);
        assert_eq!(encrypted, CLIENT_CIPHERTEXT);

        let (_, mut decrypt) = HeaderCrypto::from_session_key(SESSION_KEY).split();
        let mut decrypted = SERVER_CIPHERTEXT;
        decrypt.decrypt(&mut decrypted);
        assert_eq!(decrypted, PLAINTEXT);
    }

    #[test]
    fn chunk_boundaries_do_not_change_streams() {
        let (mut whole_encrypt, mut whole_decrypt) =
            HeaderCrypto::from_session_key(SESSION_KEY).split();
        let (mut chunked_encrypt, mut chunked_decrypt) =
            HeaderCrypto::from_session_key(SESSION_KEY).split();

        let mut encrypted_whole = PLAINTEXT;
        whole_encrypt.encrypt(&mut encrypted_whole);
        let mut encrypted_chunked = PLAINTEXT;
        for chunk in encrypted_chunked.chunks_mut(3) {
            chunked_encrypt.encrypt(chunk);
        }
        assert_eq!(encrypted_chunked, encrypted_whole);

        let mut decrypted_whole = SERVER_CIPHERTEXT;
        whole_decrypt.decrypt(&mut decrypted_whole);
        let mut decrypted_chunked = SERVER_CIPHERTEXT;
        for chunk in decrypted_chunked.chunks_mut(5) {
            chunked_decrypt.decrypt(chunk);
        }
        assert_eq!(decrypted_chunked, decrypted_whole);
        assert_eq!(decrypted_chunked, PLAINTEXT);
    }

    #[test]
    fn directions_use_separate_streams() {
        assert_ne!(CLIENT_CIPHERTEXT, SERVER_CIPHERTEXT);

        let mut crypto = HeaderCrypto::from_session_key(SESSION_KEY);
        let mut client_bytes = PLAINTEXT;
        crypto.encrypter().encrypt(&mut client_bytes);
        assert_eq!(client_bytes, CLIENT_CIPHERTEXT);

        let mut wrong_direction = CLIENT_CIPHERTEXT;
        crypto.decrypter().decrypt(&mut wrong_direction);
        assert_ne!(wrong_direction, PLAINTEXT);
    }
}
