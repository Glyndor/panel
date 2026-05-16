use sha2::{Digest, Sha256};

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex_encode(&h.finalize())
}

pub fn email_hash(email: &str, pepper: &str) -> String {
    let mut h = Sha256::new();
    h.update(email.to_lowercase().as_bytes());
    h.update(pepper.as_bytes());
    hex_encode(&h.finalize())
}

pub fn token_hash(token: &[u8], pepper: &str) -> String {
    let mut h = Sha256::new();
    h.update(token);
    h.update(pepper.as_bytes());
    hex_encode(&h.finalize())
}

pub fn ip_hash(ip: &str) -> String {
    let digest = Sha256::digest(ip.as_bytes());
    hex_encode(&digest[..16])
}

pub fn ua_hash(ua: &str) -> String {
    let digest = Sha256::digest(ua.as_bytes());
    hex_encode(&digest[..16])
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
