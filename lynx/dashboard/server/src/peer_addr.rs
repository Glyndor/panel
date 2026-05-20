use axum::{extract::ConnectInfo, middleware::Next, response::Response};
use axum::extract::Request;
use std::net::SocketAddr;

/// Middleware: injects the TCP peer address as `X-Peer-Addr` header when the
/// header is not already set by a reverse proxy. This lets IP extraction in auth
/// handlers and middleware fall back to the real socket address instead of returning
/// an empty string when `X-Real-Ip` and `X-Forwarded-For` are absent.
pub async fn inject_peer_addr(mut req: Request, next: Next) -> Response {
    if req.headers().get("x-peer-addr").is_none() {
        if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
            let ip_str = addr.ip().to_string();
            if let Ok(val) = ip_str.parse() {
                req.headers_mut().insert("x-peer-addr", val);
            }
        }
    }
    next.run(req).await
}
