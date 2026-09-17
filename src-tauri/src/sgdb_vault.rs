//! Opaque credential vault for SteamGridDB.
//! The bearer token is AES-256-GCM sealed. The AES key is never stored —
//! it is stretched at runtime from scattered shards via thousands of SHA-256
//! rounds. Decoy shards/ciphertexts are present to frustrate static dumps.
//!
//! Note: any secret shipped inside a client can eventually be recovered by a
//! determined reverse engineer; this raises the bar far beyond `strings`/hex.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

/// Sealed bearer token (ciphertext ‖ tag). Split so it is not one contiguous blob.
const SEAL_P0: &[u8] = &[
    0xab, 0x0f, 0x2a, 0xc8, 0x7d, 0x28, 0x4e, 0x87, 0x61, 0xbc, 0xe0, 0x58, 0x4f, 0xd7, 0xf1, 0x90,
];
const SEAL_P1: &[u8] = &[
    0x6c, 0x16, 0x7c, 0xce, 0x97, 0x38, 0x68, 0xbd, 0x0e, 0xfb, 0x8d, 0x88, 0x28, 0x4e, 0x7a, 0xd0,
];
const SEAL_P2: &[u8] = &[
    0xdf, 0x73, 0xcf, 0x27, 0xca, 0xaf, 0x1e, 0xe7, 0x0f, 0xa8, 0xd0, 0x58, 0x7e, 0x39, 0x39, 0x9a,
];

const IV: &[u8] = &[
    0x7f, 0xfb, 0x8d, 0x13, 0xec, 0x95, 0xfb, 0xe1, 0x66, 0xcd, 0x07, 0xf3,
];

// Real key-schedule shards (order of consumption is non-linear).
const K_ALPHA: &[u8] = &[
    0xa1, 0xf3, 0xc9, 0xe2, 0x07, 0x5b, 0x4d, 0x8e, 0x6c, 0x1a, 0x90, 0x3f, 0x5e, 0x2b, 0x7d, 0x44,
];
const K_BETA: &[u8] = &[
    0x19, 0xd0, 0xb6, 0xa8, 0x4f, 0x3c, 0x2e, 0x91, 0x75, 0xa0, 0xc6, 0xd3, 0xe8, 0xb1, 0x4f, 0x2a,
];
const K_GAMMA: &[u8] = &[
    0x7e, 0x5c, 0x1b, 0x90, 0xa3, 0xd4, 0xf6, 0x28, 0xc0, 0xe9, 0xb5, 0xa1, 0x74, 0x3d, 0x2f, 0x08,
];
const K_DELTA: &[u8] = &[
    0xc4, 0xa9, 0xe0, 0xf1, 0x5b, 0x3d, 0x78, 0x26, 0xa1, 0xc9, 0xe4, 0xf0, 0xb7, 0xd5, 0x32, 0x61,
];

// Decoys — look like the real material; never fed into the successful path.
const D_SHARD_A: &[u8] = &[
    0xde, 0xad, 0xbe, 0xef, 0xca, 0xfe, 0xba, 0xbe, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
];
const D_SHARD_B: &[u8] = &[
    0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32, 0x10,
];
const D_SEAL: &[u8] = &[
    0x9f, 0x8e, 0x7d, 0x6c, 0x5b, 0x4a, 0x39, 0x28, 0x17, 0x06, 0xf5, 0xe4, 0xd3, 0xc2, 0xb1, 0xa0,
    0x0f, 0x1e, 0x2d, 0x3c, 0x4b, 0x5a, 0x69, 0x78, 0x87, 0x96, 0xa5, 0xb4, 0xc3, 0xd2, 0xe1, 0xf0,
    0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99,
];
const D_IV: &[u8] = &[
    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb,
];

const AAD: &[u8] = b"aether.steamgriddb.v3";
const DOMAIN: &[u8] = b"AetherSgdb.v3";
const STRETCH_ROUNDS: u32 = 4096;

#[inline(never)]
fn mix_shard(state: &mut [u8; 32], index: u8, shard: &[u8]) {
    let mut rev = shard.to_vec();
    rev.reverse();

    let mut inner = Sha256::new();
    inner.update(shard);
    inner.update([index.wrapping_mul(13).wrapping_add(7)]);
    let nested = inner.finalize();

    let mut h = Sha256::new();
    h.update(&*state);
    h.update([index ^ 0x5A]);
    h.update(&rev);
    h.update(&nested);
    let dig = h.finalize();
    state.copy_from_slice(&dig);

    rev.zeroize();
}

#[inline(never)]
fn stretch_key(mut state: [u8; 32]) -> [u8; 32] {
    for r in 0..STRETCH_ROUNDS {
        let mut h = Sha256::new();
        h.update(&state);
        h.update([(r & 0xFF) as u8, ((r >> 8) & 0xFF) as u8]);
        h.update(DOMAIN);
        // Non-linear tweak so a simple “hash N times” script is wrong.
        if r % 17 == 0 {
            h.update([0xA5, 0x5A]);
        }
        if r % 41 == 3 {
            h.update(&state[0..8]);
        }
        let dig = h.finalize();
        state.copy_from_slice(&dig);
    }
    state
}

#[inline(never)]
fn assemble_aes_key() -> [u8; 32] {
    let mut state = [0u8; 32];
    // Intentionally weird consumption order: 2 → 0 → 3 → 1
    mix_shard(&mut state, 2, K_GAMMA);
    mix_shard(&mut state, 0, K_ALPHA);
    mix_shard(&mut state, 3, K_DELTA);
    mix_shard(&mut state, 1, K_BETA);
    stretch_key(state)
}

#[inline(never)]
fn sealed_blob() -> Vec<u8> {
    let mut out = Vec::with_capacity(48);
    out.extend_from_slice(SEAL_P0);
    out.extend_from_slice(SEAL_P1);
    out.extend_from_slice(SEAL_P2);
    out
}

fn nonce_from(bytes: &[u8]) -> Nonce<aes_gcm::aead::consts::U12> {
    let arr: [u8; 12] = bytes.try_into().expect("nonce length");
    arr.into()
}

/// Dead path — looks like real decryption to static analysis / tracers.
#[inline(never)]
#[allow(dead_code)]
fn decoy_unlock() -> Option<String> {
    let mut state = [0u8; 32];
    mix_shard(&mut state, 0, D_SHARD_A);
    mix_shard(&mut state, 1, D_SHARD_B);
    let mut key = stretch_key(state);
    let cipher = Aes256Gcm::new_from_slice(&key).ok()?;
    let nonce = nonce_from(D_IV);
    let _ = cipher.decrypt(
        &nonce,
        aes_gcm::aead::Payload {
            msg: D_SEAL,
            aad: AAD,
        },
    );
    key.zeroize();
    None
}

/// Unlock the SteamGridDB bearer token into a short-lived String.
#[inline(never)]
pub fn unlock_bearer() -> String {
    // Touch decoys so they are not stripped and scanners see multiple paths.
    let _ = (D_SHARD_A, D_SHARD_B, D_SEAL, D_IV);
    if std::hint::black_box(false) {
        let _ = decoy_unlock();
    }

    let mut aes_key = assemble_aes_key();
    let cipher = match Aes256Gcm::new_from_slice(&aes_key) {
        Ok(c) => c,
        Err(_) => {
            aes_key.zeroize();
            return String::new();
        }
    };

    let nonce = nonce_from(IV);
    let sealed = sealed_blob();
    let plain = match cipher.decrypt(
        &nonce,
        aes_gcm::aead::Payload {
            msg: &sealed,
            aad: AAD,
        },
    ) {
        Ok(p) => p,
        Err(_) => {
            aes_key.zeroize();
            return String::new();
        }
    };
    aes_key.zeroize();

    let s = String::from_utf8(plain).unwrap_or_default();
    if s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return String::new();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlocks_expected_shape() {
        let s = unlock_bearer();
        assert_eq!(s.len(), 32, "unlock returned empty — seal/key mismatch");
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn live_steamgriddb_search() {
        let key = unlock_bearer();
        assert_eq!(key.len(), 32);
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("client");
        let resp = client
            .get("https://www.steamgriddb.com/api/v2/search/autocomplete/Celeste")
            .header("Authorization", format!("Bearer {key}"))
            .send()
            .expect("http request");
        assert!(
            resp.status().is_success(),
            "SteamGridDB HTTP {}",
            resp.status()
        );
        let body: serde_json::Value = resp.json().expect("json");
        assert_eq!(body["success"], true);
        let data = body["data"].as_array().expect("data array");
        assert!(!data.is_empty(), "expected search hits");
    }
}
