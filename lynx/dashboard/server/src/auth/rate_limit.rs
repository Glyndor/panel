use crate::error::{AppError, Result};
use anyhow::Context;
use redis::{aio::ConnectionManager, AsyncCommands};

const LOGIN_LIMIT: i64 = 5;
const LOGIN_WINDOW: i64 = 900;
const REGISTER_LIMIT: i64 = 3;
const REGISTER_WINDOW: i64 = 3600;

pub async fn check_login(redis: &mut ConnectionManager, ip: &str) -> Result<()> {
    check(redis, &format!("rl:login:{ip}"), LOGIN_LIMIT, LOGIN_WINDOW).await
}

pub async fn check_register(redis: &mut ConnectionManager, ip: &str) -> Result<()> {
    check(
        redis,
        &format!("rl:register:{ip}"),
        REGISTER_LIMIT,
        REGISTER_WINDOW,
    )
    .await
}

async fn check(redis: &mut ConnectionManager, key: &str, limit: i64, window: i64) -> Result<()> {
    let count: i64 = redis.incr(key, 1i64).await.context("redis INCR")?;

    if count == 1 {
        let _: () = redis.expire(key, window).await.context("redis EXPIRE")?;
    }

    if count > limit {
        let ttl: i64 = redis.ttl(key).await.unwrap_or(window);
        let retry = if ttl > 0 { ttl as u64 } else { window as u64 };
        return Err(AppError::RateLimited { retry_after: retry });
    }

    Ok(())
}
