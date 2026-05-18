pub async fn configure_domain(domain: &str, email: &str) -> anyhow::Result<()> {
    setup_nginx_container(domain).await?;
    obtain_lets_encrypt_cert(domain, email).await?;
    reload_nginx_config(domain, false).await?;
    Ok(())
}

async fn setup_nginx_container(domain: &str) -> anyhow::Result<()> {
    let compose_yaml = nginx_compose_yaml(domain, false, false);
    let compose_path = "/etc/lynx/nginx/docker-compose.yml";
    std::fs::create_dir_all("/etc/lynx/nginx")?;
    std::fs::write(compose_path, &compose_yaml)?;

    let status = tokio::process::Command::new("lynx-compose")
        .args(["up", "-d", "--remove-orphans"])
        .current_dir("/etc/lynx/nginx")
        .status()
        .await?;

    if !status.success() {
        let status2 = tokio::process::Command::new("podman-compose")
            .args(["-f", compose_path, "up", "-d"])
            .status()
            .await?;
        anyhow::ensure!(status2.success(), "nginx container failed to start");
    }

    Ok(())
}

async fn obtain_lets_encrypt_cert(domain: &str, email: &str) -> anyhow::Result<()> {
    let status = tokio::process::Command::new("certbot")
        .args([
            "certonly",
            "--webroot",
            "--webroot-path",
            "/var/lib/lynx/nginx/webroot",
            "--non-interactive",
            "--agree-tos",
            "--email",
            email,
            "-d",
            domain,
        ])
        .status()
        .await?;

    anyhow::ensure!(status.success(), "certbot failed to obtain certificate");
    Ok(())
}

pub async fn reload_nginx_config(domain: &str, hsts: bool) -> anyhow::Result<()> {
    let cert_path = format!("/etc/letsencrypt/live/{domain}/fullchain.pem");
    let has_cert = std::path::Path::new(&cert_path).exists();
    let compose_yaml = nginx_compose_yaml(domain, has_cert, hsts);

    std::fs::create_dir_all("/etc/lynx/nginx")?;
    std::fs::write("/etc/lynx/nginx/docker-compose.yml", &compose_yaml)?;
    std::fs::write(
        "/etc/lynx/nginx/nginx.conf",
        nginx_conf(domain, has_cert, hsts),
    )?;

    let _ = tokio::process::Command::new("podman")
        .args(["exec", "lynx-dashboard-nginx", "nginx", "-s", "reload"])
        .status()
        .await;

    Ok(())
}

fn nginx_compose_yaml(_domain: &str, has_cert: bool, _hsts: bool) -> String {
    let cert_mount = if has_cert {
        format!("      - /etc/letsencrypt:/etc/letsencrypt:ro")
    } else {
        String::new()
    };

    format!(
        r#"networks:
  lynx-dashboard-app:
    external: true

services:
  lynx-dashboard-nginx:
    image: docker.io/nginx:1-alpine
    container_name: lynx-dashboard-nginx
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - /etc/lynx/nginx/nginx.conf:/etc/nginx/conf.d/lynx.conf:ro
      - /var/lib/lynx/nginx/webroot:/var/www/html:ro
{cert_mount}
    networks:
      - lynx-dashboard-app
"#
    )
}

fn nginx_conf(domain: &str, has_cert: bool, hsts: bool) -> String {
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
    listen 443 ssl http2;
    server_name {domain};
    ssl_certificate /etc/letsencrypt/live/{domain}/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/{domain}/privkey.pem;
    ssl_session_timeout 1d;
    ssl_session_cache shared:MozSSL:10m;
    ssl_protocols TLSv1.3 TLSv1.2;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384;
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
