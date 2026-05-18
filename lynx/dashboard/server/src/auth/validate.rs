use crate::error::{AppError, Result};

const RESERVED_USERNAMES: &[&str] = &[
    "admin",
    "root",
    "system",
    "lynx",
    "support",
    "api",
    "null",
    "undefined",
];

pub fn username(s: &str) -> Result<()> {
    if s.len() < 3 || s.len() > 32 {
        return Err(AppError::Validation("username: 3–32 characters".into()));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(AppError::Validation(
            "username: only lowercase letters, digits, - and _".into(),
        ));
    }
    if s.starts_with(['-', '_']) || s.ends_with(['-', '_']) {
        return Err(AppError::Validation(
            "username: cannot start or end with - or _".into(),
        ));
    }
    if RESERVED_USERNAMES.contains(&s) {
        return Err(AppError::Validation("username: reserved".into()));
    }
    Ok(())
}

pub fn password(s: &str) -> Result<()> {
    if s.len() < 12 || s.len() > 30 {
        return Err(AppError::Validation("password: 12–30 characters".into()));
    }
    let has_upper = s.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = s.chars().any(|c| c.is_ascii_lowercase());
    let has_digit = s.chars().any(|c| c.is_ascii_digit());
    let has_special = s.chars().any(|c| !c.is_alphanumeric());
    if !has_upper || !has_lower || !has_digit || !has_special {
        return Err(AppError::Validation(
            "password: requires uppercase, lowercase, digit, and special character".into(),
        ));
    }
    Ok(())
}

pub fn email(s: &str) -> Result<()> {
    let s = s.trim();
    if s.len() > 254 {
        return Err(AppError::Validation("email: too long".into()));
    }
    let parts: Vec<&str> = s.splitn(2, '@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(AppError::Validation("email: invalid format".into()));
    }
    if !parts[1].contains('.') {
        return Err(AppError::Validation("email: invalid domain".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- username ---

    #[test]
    fn username_valid() {
        assert!(username("alice").is_ok());
        assert!(username("alice-bob").is_ok());
        assert!(username("alice_bob").is_ok());
        assert!(username("alice123").is_ok());
        assert!(username("abc").is_ok());
        assert!(username(&"a".repeat(32)).is_ok());
    }

    #[test]
    fn username_too_short() {
        assert!(username("ab").is_err());
        assert!(username("").is_err());
    }

    #[test]
    fn username_too_long() {
        assert!(username(&"a".repeat(33)).is_err());
    }

    #[test]
    fn username_invalid_chars() {
        assert!(username("Alice").is_err()); // uppercase
        assert!(username("ali ce").is_err()); // space
        assert!(username("ali@ce").is_err()); // @
        assert!(username("ali.ce").is_err()); // dot
    }

    #[test]
    fn username_bad_edge_chars() {
        assert!(username("-alice").is_err());
        assert!(username("alice-").is_err());
        assert!(username("_alice").is_err());
        assert!(username("alice_").is_err());
    }

    #[test]
    fn username_reserved() {
        for r in [
            "admin",
            "root",
            "system",
            "lynx",
            "support",
            "api",
            "null",
            "undefined",
        ] {
            assert!(username(r).is_err(), "{r} should be reserved");
        }
    }

    // --- password ---

    #[test]
    fn password_valid() {
        assert!(password("Abcdef1234!@").is_ok());
        assert!(password("Hunter2#Correct").is_ok());
    }

    #[test]
    fn password_too_short() {
        assert!(password("Ab1!").is_err());
        assert!(password("Ab1!defghijk").is_ok()); // exactly 12 → ok
    }

    #[test]
    fn password_too_long() {
        let p31 = "Aa1!".repeat(7) + "Aa1!Aa1"; // 31 chars
        assert!(password(&p31).is_err());
    }

    #[test]
    fn password_boundary_30() {
        let p = "Aa1!".repeat(7) + "Aa"; // 30 chars
        assert!(password(&p).is_ok());
    }

    #[test]
    fn password_missing_uppercase() {
        assert!(password("abcdef1234!@").is_err());
    }

    #[test]
    fn password_missing_lowercase() {
        assert!(password("ABCDEF1234!@").is_err());
    }

    #[test]
    fn password_missing_digit() {
        assert!(password("AbcdefGhij!@").is_err());
    }

    #[test]
    fn password_missing_special() {
        assert!(password("Abcdef123456").is_err());
    }

    // --- email ---

    #[test]
    fn email_valid() {
        assert!(email("user@example.com").is_ok());
        assert!(email("USER@EXAMPLE.COM").is_ok());
        assert!(email("  user@example.com  ").is_ok()); // trimmed
        assert!(email("a@b.c").is_ok());
    }

    #[test]
    fn email_no_at() {
        assert!(email("userexample.com").is_err());
    }

    #[test]
    fn email_no_domain_dot() {
        assert!(email("user@localhost").is_err());
    }

    #[test]
    fn email_empty_local() {
        assert!(email("@example.com").is_err());
    }

    #[test]
    fn email_too_long() {
        let long = "a".repeat(250) + "@x.co";
        assert!(email(&long).is_err());
    }
}
