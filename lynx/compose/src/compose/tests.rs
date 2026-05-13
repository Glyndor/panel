//! Unit tests for compose file parsing and service ordering.

use super::{parse_str_raw as parse_str, resolve_order};
use crate::compose::types::*;

// ---------------------------------------------------------------------------
// Parser — basic fields
// ---------------------------------------------------------------------------

#[test]
fn parse_minimal() {
    let yaml = r#"
services:
  web:
    image: nginx:alpine
"#;
    let file = parse_str(yaml).unwrap();
    assert!(file.services.contains_key("web"));
    assert_eq!(file.services["web"].image.as_deref(), Some("nginx:alpine"));
}

#[test]
fn parse_env_as_list() {
    let yaml = r#"
services:
  app:
    image: node:20
    environment:
      - NODE_ENV=production
      - PORT=3000
      - SECRET
"#;
    let file = parse_str(yaml).unwrap();
    let env = file.services["app"].environment.to_map();
    assert_eq!(env["NODE_ENV"].as_deref(), Some("production"));
    assert_eq!(env["PORT"].as_deref(), Some("3000"));
    assert!(env.contains_key("SECRET"));
}

#[test]
fn parse_env_as_map() {
    let yaml = r#"
services:
  app:
    image: node:20
    environment:
      NODE_ENV: production
      PORT: 3000
"#;
    let file = parse_str(yaml).unwrap();
    let env = file.services["app"].environment.to_map();
    assert_eq!(env["NODE_ENV"].as_deref(), Some("production"));
    assert_eq!(env["PORT"].as_deref(), Some("3000"));
}

#[test]
fn parse_depends_on_as_list() {
    let yaml = r#"
services:
  app:
    image: node:20
    depends_on:
      - db
      - redis
  db:
    image: postgres:17
  redis:
    image: redis:8
"#;
    let file = parse_str(yaml).unwrap();
    let deps = file.services["app"].depends_on.service_names();
    assert!(deps.contains(&"db".to_string()));
    assert!(deps.contains(&"redis".to_string()));
}

#[test]
fn parse_depends_on_as_map_with_condition() {
    let yaml = r#"
services:
  app:
    image: node:20
    depends_on:
      db:
        condition: service_healthy
  db:
    image: postgres:17
"#;
    let file = parse_str(yaml).unwrap();
    let deps = file.services["app"].depends_on.service_names();
    assert!(deps.contains(&"db".to_string()));
    let condition = file.services["app"].depends_on.condition_for("db");
    assert!(matches!(condition, ServiceCondition::ServiceHealthy));
}

#[test]
fn parse_volumes_short() {
    let yaml = r#"
services:
  db:
    image: postgres:17
    volumes:
      - ./data:/var/lib/postgresql/data
      - pgdata:/var/lib/postgresql/data2
volumes:
  pgdata:
"#;
    let file = parse_str(yaml).unwrap();
    assert_eq!(file.services["db"].volumes.len(), 2);
    assert!(file.volumes.contains_key("pgdata"));
}

#[test]
fn parse_ports_short() {
    let yaml = r#"
services:
  web:
    image: nginx
    ports:
      - "80:80"
      - "443:443"
"#;
    let file = parse_str(yaml).unwrap();
    assert_eq!(file.services["web"].ports.len(), 2);
}

#[test]
fn parse_networks() {
    let yaml = r#"
services:
  web:
    image: nginx
    networks:
      - frontend
networks:
  frontend:
    driver: bridge
"#;
    let file = parse_str(yaml).unwrap();
    assert!(file.networks.contains_key("frontend"));
    assert_eq!(file.services["web"].networks.names(), vec!["frontend"]);
}

#[test]
fn parse_secrets() {
    let yaml = r#"
secrets:
  db_password:
    file: ./secrets/db_password.txt
  jwt_secret:
    external: true
"#;
    let file = parse_str(yaml).unwrap();
    assert_eq!(
        file.secrets["db_password"].file.as_deref(),
        Some("./secrets/db_password.txt")
    );
    assert_eq!(file.secrets["jwt_secret"].external, Some(true));
}

#[test]
fn parse_command_shell() {
    let yaml = r#"
services:
  app:
    image: node:20
    command: "node server.js --port 3000"
"#;
    let file = parse_str(yaml).unwrap();
    let cmd = file.services["app"].command.as_ref().unwrap();
    let exec = cmd.to_exec();
    assert_eq!(exec[0], "sh");
    assert_eq!(exec[1], "-c");
}

#[test]
fn parse_command_exec() {
    let yaml = r#"
services:
  app:
    image: node:20
    command: ["node", "server.js"]
"#;
    let file = parse_str(yaml).unwrap();
    let cmd = file.services["app"].command.as_ref().unwrap();
    let exec = cmd.to_exec();
    assert_eq!(exec, vec!["node", "server.js"]);
}

#[test]
fn parse_healthcheck() {
    let yaml = r#"
services:
  db:
    image: postgres:17
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 5s
      timeout: 3s
      retries: 10
"#;
    let file = parse_str(yaml).unwrap();
    let hc = file.services["db"].healthcheck.as_ref().unwrap();
    assert!(hc.retries == Some(10));
}

#[test]
fn parse_empty_file() {
    let file = parse_str("services: {}").unwrap();
    assert!(file.services.is_empty());
}

#[test]
fn parse_invalid_yaml() {
    let result = parse_str("services: [invalid: yaml: here");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Parser — new fields
// ---------------------------------------------------------------------------

#[test]
fn parse_sysctls_as_list() {
    let yaml = r#"
services:
  app:
    image: alpine
    sysctls:
      - net.core.somaxconn=1024
      - net.ipv4.ip_forward=1
"#;
    let file = parse_str(yaml).unwrap();
    let sysctls = file.services["app"].sysctls.to_map();
    assert_eq!(sysctls.get("net.core.somaxconn").map(|s| s.as_str()), Some("1024"));
    assert_eq!(sysctls.get("net.ipv4.ip_forward").map(|s| s.as_str()), Some("1"));
}

#[test]
fn parse_sysctls_as_map() {
    let yaml = r#"
services:
  app:
    image: alpine
    sysctls:
      net.core.somaxconn: "1024"
"#;
    let file = parse_str(yaml).unwrap();
    let sysctls = file.services["app"].sysctls.to_map();
    assert_eq!(sysctls.get("net.core.somaxconn").map(|s| s.as_str()), Some("1024"));
}

#[test]
fn parse_ulimits_as_number() {
    let yaml = r#"
services:
  app:
    image: alpine
    ulimits:
      nofile: 1024
"#;
    let file = parse_str(yaml).unwrap();
    let ul = &file.services["app"].ulimits["nofile"];
    assert_eq!(ul.soft(), 1024);
    assert_eq!(ul.hard(), 1024);
}

#[test]
fn parse_ulimits_as_object() {
    let yaml = r#"
services:
  app:
    image: alpine
    ulimits:
      nofile:
        soft: 1024
        hard: 65536
"#;
    let file = parse_str(yaml).unwrap();
    let ul = &file.services["app"].ulimits["nofile"];
    assert_eq!(ul.soft(), 1024);
    assert_eq!(ul.hard(), 65536);
}

#[test]
fn parse_logging_config() {
    let yaml = r#"
services:
  app:
    image: alpine
    logging:
      driver: json-file
      options:
        max-size: 10m
        max-file: "3"
"#;
    let file = parse_str(yaml).unwrap();
    let logging = file.services["app"].logging.as_ref().unwrap();
    assert_eq!(logging.driver.as_deref(), Some("json-file"));
    assert_eq!(logging.options.get("max-size").map(|s| s.as_str()), Some("10m"));
}

#[test]
fn parse_deploy_config() {
    let yaml = r#"
services:
  app:
    image: alpine
    deploy:
      replicas: 3
      resources:
        limits:
          cpus: "0.5"
          memory: 128M
"#;
    let file = parse_str(yaml).unwrap();
    let deploy = file.services["app"].deploy.as_ref().unwrap();
    assert_eq!(deploy.replicas, Some(3));
    let limits = deploy.resources.as_ref().unwrap().limits.as_ref().unwrap();
    assert_eq!(limits.cpus.as_deref(), Some("0.5"));
    assert_eq!(limits.memory.as_deref(), Some("128M"));
}

#[test]
fn parse_network_mode_host() {
    let yaml = r#"
services:
  app:
    image: alpine
    network_mode: host
"#;
    let file = parse_str(yaml).unwrap();
    assert_eq!(
        file.services["app"].network_mode.as_deref(),
        Some("host")
    );
}

#[test]
fn parse_profiles() {
    let yaml = r#"
services:
  debug:
    image: alpine
    profiles:
      - debug
      - dev
"#;
    let file = parse_str(yaml).unwrap();
    let profiles = &file.services["debug"].profiles;
    assert!(profiles.contains(&"debug".to_string()));
    assert!(profiles.contains(&"dev".to_string()));
}

#[test]
fn parse_secrets_with_file_and_external() {
    let yaml = r#"
services:
  app:
    image: alpine
    secrets:
      - my_secret
      - ext_secret
secrets:
  my_secret:
    file: ./secret.txt
  ext_secret:
    external: true
"#;
    let file = parse_str(yaml).unwrap();
    assert_eq!(file.secrets["my_secret"].file.as_deref(), Some("./secret.txt"));
    assert_eq!(file.secrets["ext_secret"].external, Some(true));
    assert!(file.services["app"].secrets.contains(&"my_secret".to_string()));
}

#[test]
fn parse_env_file_as_string() {
    let yaml = r#"
services:
  app:
    image: alpine
    env_file: .env
"#;
    let file = parse_str(yaml).unwrap();
    let list = file.services["app"].env_file.to_list();
    assert_eq!(list, vec![".env"]);
}

#[test]
fn parse_env_file_as_list() {
    let yaml = r#"
services:
  app:
    image: alpine
    env_file:
      - .env
      - .env.local
"#;
    let file = parse_str(yaml).unwrap();
    let list = file.services["app"].env_file.to_list();
    assert_eq!(list.len(), 2);
    assert!(list.contains(&".env.local".to_string()));
}

#[test]
fn parse_restart_policies() {
    let policies = [
        ("no", "no"),
        ("always", "always"),
        ("on-failure", "on-failure"),
        ("unless-stopped", "unless-stopped"),
    ];
    for (yaml_val, _label) in &policies {
        let yaml = format!(
            "services:\n  app:\n    image: alpine\n    restart: {yaml_val}\n"
        );
        let file = parse_str(&yaml).unwrap();
        assert!(file.services["app"].restart.is_some());
    }
}

#[test]
fn parse_labels_as_list() {
    let yaml = r#"
services:
  app:
    image: alpine
    labels:
      - "com.example.env=prod"
      - "com.example.version=1.0"
"#;
    let file = parse_str(yaml).unwrap();
    let labels = file.services["app"].labels.to_map();
    assert_eq!(labels.get("com.example.env").map(|s| s.as_str()), Some("prod"));
}

#[test]
fn parse_labels_as_map() {
    let yaml = r#"
services:
  app:
    image: alpine
    labels:
      com.example.env: prod
      com.example.version: "1.0"
"#;
    let file = parse_str(yaml).unwrap();
    let labels = file.services["app"].labels.to_map();
    assert_eq!(labels.get("com.example.env").map(|s| s.as_str()), Some("prod"));
}

#[test]
fn parse_extra_hosts() {
    let yaml = r#"
services:
  app:
    image: alpine
    extra_hosts:
      - "somehost:162.242.195.82"
      - "otherhost:50.31.209.229"
"#;
    let file = parse_str(yaml).unwrap();
    assert_eq!(file.services["app"].extra_hosts.len(), 2);
}

#[test]
fn parse_tty_and_stdin_open() {
    let yaml = r#"
services:
  app:
    image: alpine
    tty: true
    stdin_open: true
"#;
    let file = parse_str(yaml).unwrap();
    assert_eq!(file.services["app"].tty, Some(true));
    assert_eq!(file.services["app"].stdin_open, Some(true));
}

#[test]
fn parse_privileged_and_init() {
    let yaml = r#"
services:
  app:
    image: alpine
    privileged: true
    init: true
"#;
    let file = parse_str(yaml).unwrap();
    assert_eq!(file.services["app"].privileged, Some(true));
    assert_eq!(file.services["app"].init, Some(true));
}

#[test]
fn parse_stop_signal() {
    let yaml = r#"
services:
  app:
    image: alpine
    stop_signal: SIGTERM
"#;
    let file = parse_str(yaml).unwrap();
    assert_eq!(file.services["app"].stop_signal.as_deref(), Some("SIGTERM"));
}

#[test]
fn parse_dns() {
    let yaml = r#"
services:
  app:
    image: alpine
    dns:
      - 8.8.8.8
      - 8.8.4.4
"#;
    let file = parse_str(yaml).unwrap();
    let dns = file.services["app"].dns.to_list();
    assert!(dns.contains(&"8.8.8.8".to_string()));
}

#[test]
fn parse_cap_add_and_drop() {
    let yaml = r#"
services:
  app:
    image: alpine
    cap_add:
      - NET_ADMIN
    cap_drop:
      - ALL
"#;
    let file = parse_str(yaml).unwrap();
    assert!(file.services["app"].cap_add.contains(&"NET_ADMIN".to_string()));
    assert!(file.services["app"].cap_drop.contains(&"ALL".to_string()));
}

// ---------------------------------------------------------------------------
// resolve_order
// ---------------------------------------------------------------------------

#[test]
fn order_no_deps() {
    let yaml = r#"
services:
  a:
    image: alpine
  b:
    image: alpine
  c:
    image: alpine
"#;
    let file = parse_str(yaml).unwrap();
    let order = resolve_order(&file).unwrap();
    assert_eq!(order.len(), 3);
}

#[test]
fn order_linear_chain() {
    let yaml = r#"
services:
  c:
    image: alpine
    depends_on: [b]
  b:
    image: alpine
    depends_on: [a]
  a:
    image: alpine
"#;
    let file = parse_str(yaml).unwrap();
    let order = resolve_order(&file).unwrap();
    let pos = |s: &str| order.iter().position(|x| x == s).unwrap();
    assert!(pos("a") < pos("b"));
    assert!(pos("b") < pos("c"));
}

#[test]
fn order_diamond() {
    let yaml = r#"
services:
  app:
    image: alpine
    depends_on: [api, worker]
  api:
    image: alpine
    depends_on: [db]
  worker:
    image: alpine
    depends_on: [db]
  db:
    image: alpine
"#;
    let file = parse_str(yaml).unwrap();
    let order = resolve_order(&file).unwrap();
    let pos = |s: &str| order.iter().position(|x| x == s).unwrap();
    assert!(pos("db") < pos("api"));
    assert!(pos("db") < pos("worker"));
    assert!(pos("api") < pos("app"));
    assert!(pos("worker") < pos("app"));
}

#[test]
fn order_circular_dependency() {
    let yaml = r#"
services:
  a:
    image: alpine
    depends_on: [b]
  b:
    image: alpine
    depends_on: [a]
"#;
    let file = parse_str(yaml).unwrap();
    let result = resolve_order(&file);
    assert!(result.is_err());
}

#[test]
fn order_missing_dependency() {
    let yaml = r#"
services:
  a:
    image: alpine
    depends_on: [nonexistent]
"#;
    let file = parse_str(yaml).unwrap();
    let result = resolve_order(&file);
    assert!(result.is_err());
}
