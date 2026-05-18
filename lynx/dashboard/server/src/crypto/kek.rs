use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use anyhow::Result;
use rand::Rng;
use zeroize::Zeroizing;

pub fn gen_dek() -> Zeroizing<[u8; 32]> {
    let mut dek = [0u8; 32];
    rand::rng().fill_bytes(&mut dek);
    Zeroizing::new(dek)
}

pub fn encrypt_dek(dek: &[u8; 32], kek: &[u8; 32]) -> Result<Vec<u8>> {
    encrypt_aes_gcm(dek, kek)
}

pub fn decrypt_dek(ciphertext: &[u8], kek: &[u8; 32]) -> Result<Zeroizing<[u8; 32]>> {
    let plain = decrypt_aes_gcm(ciphertext, kek)?;
    let arr: [u8; 32] = plain
        .try_into()
        .map_err(|_| anyhow::anyhow!("DEK wrong length after decrypt"))?;
    Ok(Zeroizing::new(arr))
}

pub fn encrypt_with_dek(plaintext: &[u8], dek: &[u8; 32]) -> Result<Vec<u8>> {
    encrypt_aes_gcm(plaintext, dek)
}

pub fn decrypt_with_dek(ciphertext: &[u8], dek: &[u8; 32]) -> Result<Vec<u8>> {
    decrypt_aes_gcm(ciphertext, dek)
}

fn encrypt_aes_gcm(plaintext: &[u8], key_bytes: &[u8; 32]) -> Result<Vec<u8>> {
    let key = Key::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut aes_gcm::aead::OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("AES-GCM encrypt: {e}"))?;
    // nonce (12 bytes) || ciphertext
    let mut out = nonce.to_vec();
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

fn decrypt_aes_gcm(data: &[u8], key_bytes: &[u8; 32]) -> Result<Vec<u8>> {
    if data.len() < 12 {
        anyhow::bail!("ciphertext too short");
    }
    let (nonce_bytes, ciphertext) = data.split_at(12);
    let key = Key::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("AES-GCM decrypt: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_kek() -> [u8; 32] {
        [0x11u8; 32]
    }

    #[test]
    fn dek_encrypt_decrypt_roundtrip() {
        let kek = test_kek();
        let dek = gen_dek();
        let ct = encrypt_dek(&dek, &kek).expect("encrypt");
        let recovered = decrypt_dek(&ct, &kek).expect("decrypt");
        assert_eq!(*dek, *recovered);
    }

    #[test]
    fn dek_wrong_key_fails() {
        let kek = test_kek();
        let dek = gen_dek();
        let ct = encrypt_dek(&dek, &kek).expect("encrypt");
        let bad_kek = [0x22u8; 32];
        assert!(decrypt_dek(&ct, &bad_kek).is_err(), "wrong key must fail");
    }

    #[test]
    fn data_encrypt_decrypt_roundtrip() {
        let dek: [u8; 32] = [0x33u8; 32];
        let plaintext = b"hello lynx world";
        let ct = encrypt_with_dek(plaintext, &dek).expect("encrypt");
        let recovered = decrypt_with_dek(&ct, &dek).expect("decrypt");
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn nonce_prepended_makes_ciphertext_longer_than_plaintext() {
        let dek: [u8; 32] = [0x44u8; 32];
        let plaintext = b"test";
        let ct = encrypt_with_dek(plaintext, &dek).expect("encrypt");
        // nonce (12) + GCM tag (16) + plaintext length
        assert!(ct.len() > plaintext.len() + 12);
    }

    #[test]
    fn decrypt_too_short_fails() {
        let dek: [u8; 32] = [0x44u8; 32];
        assert!(decrypt_with_dek(&[0u8; 5], &dek).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let dek: [u8; 32] = [0x55u8; 32];
        let plaintext = b"secret data";
        let mut ct = encrypt_with_dek(plaintext, &dek).expect("encrypt");
        ct[20] ^= 0xff; // flip a byte in the ciphertext
        assert!(decrypt_with_dek(&ct, &dek).is_err(), "tamper must fail");
    }

    #[test]
    fn each_encryption_uses_different_nonce() {
        let dek: [u8; 32] = [0x66u8; 32];
        let plaintext = b"same input";
        let ct1 = encrypt_with_dek(plaintext, &dek).expect("ct1");
        let ct2 = encrypt_with_dek(plaintext, &dek).expect("ct2");
        // Different nonces → different ciphertexts
        assert_ne!(ct1, ct2, "nonce must be random per encryption");
    }
}
