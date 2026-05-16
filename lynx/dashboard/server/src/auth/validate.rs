use crate::error::{AppError, Result};

const RESERVED_USERNAMES: &[&str] = &[
    "admin", "root", "system", "lynx", "support", "api", "null", "undefined",
];

pub fn username(s: &str) -> Result<()> {
    if s.len() < 3 || s.len() > 32 {
        return Err(AppError::Validation("username: 3–32 characters".into()));
    }
    if !s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
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
