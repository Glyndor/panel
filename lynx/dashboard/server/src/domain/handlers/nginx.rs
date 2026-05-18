//! nginx config generation — pure functions, no OS calls.
//! All deployment goes through the local agent via signed commands.

pub const NGINX_IMAGE: &str =
    "docker.io/library/nginx@sha256:ceba1c7f1e2c42e5f43c9fa55e74ef90a1d08e7fde12f25e2a6706f4c80e0428";

/// Path where the agent stores externally-uploaded certs.
pub fn custom_cert_path(domain: &str) -> String {
    format!("/etc/lynx/nginx/certs/{domain}/fullchain.pem")
}

pub fn custom_key_path(domain: &str) -> String {
    format!("/etc/lynx/nginx/certs/{domain}/privkey.pem")
}

/// Generate nginx config. When `cert_path` is Some, uses a custom cert path
/// instead of the Let's Encrypt path.
pub fn nginx_conf_with_cert(domain: &str, cert_path: &str, key_path: &str, hsts: bool) -> String {
    let hsts_header = if hsts {
        "    add_header Strict-Transport-Security \"max-age=63072000; includeSubDomains\" always;\n"
    } else {
        ""
    };

    format!(
        r#"server {{
    listen 80;
    server_name {domain};
    location /.well-known/acme-challenge/ {{
        root /var/www/html;
    }}
    location / {{
        return 301 https://$host$request_uri;
    }}
}}

server {{
    listen 443 ssl;
    http2 on;
    server_name {domain};
    ssl_certificate {cert_path};
    ssl_certificate_key {key_path};
    ssl_session_timeout 1d;
    ssl_session_cache shared:MozSSL:10m;
    ssl_protocols TLSv1.3;
    ssl_prefer_server_ciphers off;
{hsts_header}
    location / {{
        proxy_pass http://lynx-dashboard-frontend:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }}
}}
"#
    )
}

pub fn nginx_conf(domain: &str, has_cert: bool, hsts: bool) -> String {
    let hsts_header = if hsts && has_cert {
        "    add_header Strict-Transport-Security \"max-age=63072000; includeSubDomains\" always;\n"
    } else {
        ""
    };

    if has_cert {
        format!(
            r#"server {{
    listen 80;
    server_name {domain};
    location /.well-known/acme-challenge/ {{
        root /var/www/html;
    }}
    location / {{
        return 301 https://$host$request_uri;
    }}
}}

server {{
    listen 443 ssl;
    http2 on;
    server_name {domain};
    ssl_certificate /etc/letsencrypt/live/{domain}/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/{domain}/privkey.pem;
    ssl_session_timeout 1d;
    ssl_session_cache shared:MozSSL:10m;
    ssl_protocols TLSv1.3;
    ssl_prefer_server_ciphers off;
{hsts_header}
    location / {{
        proxy_pass http://lynx-dashboard-frontend:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }}
}}
"#
        )
    } else {
        format!(
            r#"server {{
    listen 80;
    server_name {domain};
    location /.well-known/acme-challenge/ {{
        root /var/www/html;
    }}
    location / {{
        proxy_pass http://lynx-dashboard-frontend:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }}
}}
"#
        )
    }
}
