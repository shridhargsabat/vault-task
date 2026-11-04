//! VaultTask cryptography: Argon2id key derivation + AES-256-GCM payload encryption.
//!
//! The 256-bit [`MasterKey`] lives only in Rust memory, wrapped in
//! `zeroize` so it is wiped the instant the vault locks.

use aes_gcm::{
    AeadCore, Aes256Gcm, Key, KeyInit,
    aead::{Aead, OsRng},
};
use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Argon2id parameters per VaultTask spec.
const M_COST_KIB: u32 = 65536;
const T_COST: u32 = 3;
const P_COST: u32 = 4;

/// In-memory master key. Never serialized, never sent to the WebView.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct MasterKey(pub [u8; 32]);

impl MasterKey {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Fixed verification plaintext encrypted at setup time and decrypted on
/// unlock to prove the password is correct without storing a hash.
pub const VERIFIER_PLAINTEXT: &[u8] = b"vault-task-verifier-v1";

/// Derive a 256-bit key from a password/PIN and salt using Argon2id.
pub fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    if password.is_empty() {
        return Err("password must not be empty".to_string());
    }
    if salt.len() < 16 {
        return Err("salt must be at least 16 bytes".to_string());
    }
    let params = Params::new(M_COST_KIB, T_COST, P_COST, Some(32))
        .map_err(|e| format!("argon2 params: {e}"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = [0u8; 32];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut out)
        .map_err(|e| format!("argon2 derive: {e}"))?;
    Ok(out)
}

/// Generate 16 cryptographically random salt bytes.
pub fn generate_salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    use rand::RngCore;
    OsRng.fill_bytes(&mut salt);
    salt
}

/// Encrypt plaintext with AES-256-GCM under `key`.
/// Returns `(ciphertext, nonce)` with a fresh random 96-bit nonce.
pub fn encrypt_bytes(key: &[u8; 32], plaintext: &[u8]) -> Result<(Vec<u8>, [u8; 12]), String> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| format!("aes-gcm encrypt: {e}"))?;
    let mut nonce_bytes = [0u8; 12];
    nonce_bytes.copy_from_slice(&nonce);
    Ok((ciphertext, nonce_bytes))
}

/// Decrypt AES-256-GCM `ciphertext` produced by [`encrypt_bytes`].
pub fn decrypt_bytes(
    key: &[u8; 32],
    ciphertext: &[u8],
    nonce: &[u8; 12],
) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = aes_gcm::Nonce::from_slice(nonce);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "decryption failed: wrong key or corrupted data".to_string())
}

/// Verify raw 32-byte key material against a stored verifier (the OS
/// keyring quick-unlock path — no password derivation involved).
pub fn verify_key_bytes(
    key: &[u8; 32],
    verifier_nonce: &[u8],
    verifier_ciphertext: &[u8],
) -> Result<(), String> {
    if verifier_nonce.len() != 12 {
        return Err("vault verifier is corrupted".to_string());
    }
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(verifier_nonce);
    let plain = decrypt_bytes(key, verifier_ciphertext, &nonce)
        .map_err(|_| "stored OS keyring key is invalid (re-enable quick-unlock)".to_string())?;
    if plain.as_slice() != VERIFIER_PLAINTEXT {
        return Err("stored OS keyring key is invalid (re-enable quick-unlock)".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_key_is_deterministic_and_salted() {
        let salt_a = [7u8; 16];
        let salt_b = [8u8; 16];
        let k1 = derive_key("correct-horse-123", &salt_a).unwrap();
        let k2 = derive_key("correct-horse-123", &salt_a).unwrap();
        let k3 = derive_key("correct-horse-123", &salt_b).unwrap();
        let k4 = derive_key("wrong-password", &salt_a).unwrap();
        assert_eq!(k1, k2, "same password+salt must derive same key");
        assert_ne!(k1, k3, "different salt must derive different key");
        assert_ne!(k1, k4, "different password must derive different key");
    }

    #[test]
    fn derive_key_rejects_empty_password_and_short_salt() {
        assert!(derive_key("", &[0u8; 16]).is_err());
        assert!(derive_key("pin123", &[0u8; 8]).is_err());
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = derive_key("test-password", &[1u8; 16]).unwrap();
        let plaintext = "Buy milk 🥛\n- oat\n- whole".as_bytes();
        let (ct, nonce) = encrypt_bytes(&key, plaintext).unwrap();
        assert_ne!(ct, plaintext);
        let back = decrypt_bytes(&key, &ct, &nonce).unwrap();
        assert_eq!(back, plaintext);
    }

    #[test]
    fn decrypt_fails_on_wrong_key_or_tampered_data() {
        let key = derive_key("right", &[2u8; 16]).unwrap();
        let wrong = derive_key("wrong", &[2u8; 16]).unwrap();
        let (mut ct, nonce) = encrypt_bytes(&key, b"secret task title").unwrap();
        assert!(decrypt_bytes(&wrong, &ct, &nonce).is_err());
        ct[0] ^= 0xFF;
        assert!(decrypt_bytes(&key, &ct, &nonce).is_err());
    }

    #[test]
    fn nonces_are_unique_per_encryption() {
        let key = derive_key("nonce-test", &[3u8; 16]).unwrap();
        let (_, n1) = encrypt_bytes(&key, b"same plaintext").unwrap();
        let (_, n2) = encrypt_bytes(&key, b"same plaintext").unwrap();
        assert_ne!(n1, n2, "each encryption must use a fresh nonce");
    }
}
