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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_hash_deterministic_and_lowercase() {
        let h1 = email_hash("User@Example.COM", "pepper");
        let h2 = email_hash("user@example.com", "pepper");
        assert_eq!(h1, h2, "email_hash must be case-insensitive");
    }

    #[test]
    fn email_hash_pepper_matters() {
        let h1 = email_hash("user@example.com", "pepper1");
        let h2 = email_hash("user@example.com", "pepper2");
        assert_ne!(h1, h2, "different pepper must produce different hash");
    }

    #[test]
    fn email_hash_is_hex() {
        let h = email_hash("user@example.com", "pepper");
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()), "must be hex");
        assert_eq!(h.len(), 64, "sha256 → 32 bytes → 64 hex chars");
    }

    #[test]
    fn token_hash_deterministic() {
        let tok = b"random-token-bytes";
        let h1 = token_hash(tok, "pepper");
        let h2 = token_hash(tok, "pepper");
        assert_eq!(h1, h2);
    }

    #[test]
    fn token_hash_pepper_matters() {
        let tok = b"token";
        assert_ne!(token_hash(tok, "p1"), token_hash(tok, "p2"));
    }

    #[test]
    fn ip_hash_length() {
        let h = ip_hash("192.168.1.1");
        assert_eq!(h.len(), 32, "16 bytes → 32 hex chars");
    }

    #[test]
    fn ua_hash_length() {
        let h = ua_hash("Mozilla/5.0");
        assert_eq!(h.len(), 32);
    }

    #[test]
    fn sha256_hex_known_value() {
        // sha256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let h = sha256_hex(b"");
        assert_eq!(
            h,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
