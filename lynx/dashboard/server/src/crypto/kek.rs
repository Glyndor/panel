use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit},
    Aes256Gcm, Key, Nonce,
};
use anyhow::Result;
use rand::rngs::OsRng;
use zeroize::Zeroizing;

pub fn gen_dek() -> Zeroizing<[u8; 32]> {
    let mut dek = [0u8; 32];
    rand::RngCore::fill_bytes(&mut OsRng, &mut dek);
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
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
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
