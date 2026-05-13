use lynx_compose::compose::types::*;
use lynx_compose::parse_str;

#[test]
fn minimal() {
    let file = parse_str("services:\n  web:\n    image: nginx:alpine").unwrap();
    assert!(file.services.contains_key("web"));
    assert_eq!(file.services["web"].image.as_deref(), Some("nginx:alpine"));
}

#[test]
fn empty_services() {
    assert!(parse_str("services: {}").unwrap().services.is_empty());
}

#[test]
fn invalid_yaml() {
    assert!(parse_str("services: [invalid: yaml: here").is_err());
}

#[test]
fn env_as_list() {
    let yaml = r#"
services:
  app:
    image: node:20
    environment:
      - NODE_ENV=production
      - PORT=3000
      - SECRET
"#;
    let env = parse_str(yaml).unwrap().services["app"].environment.to_map();
    assert_eq!(env["NODE_ENV"].as_deref(), Some("production"));
    assert_eq!(env["PORT"].as_deref(), Some("3000"));
    assert!(env.contains_key("SECRET"));
}

#[test]
fn env_as_map() {
    let yaml = r#"
services:
  app:
    image: node:20
    environment:
      NODE_ENV: production
      PORT: 3000
"#;
    let env = parse_str(yaml).unwrap().services["app"].environment.to_map();
    assert_eq!(env["NODE_ENV"].as_deref(), Some("production"));
    assert_eq!(env["PORT"].as_deref(), Some("3000"));
}

#[test]
fn command_shell() {
    let yaml = r#"
services:
  app:
    image: node:20
    command: "node server.js --port 3000"
"#;
    let exec = parse_str(yaml).unwrap().services["app"]
        .command
        .as_ref()
        .unwrap()
        .to_exec();
    assert_eq!(exec[0], "sh");
    assert_eq!(exec[1], "-c");
}

#[test]
fn command_exec() {
    let yaml = r#"
services:
  app:
    image: node:20
    command: ["node", "server.js"]
"#;
    let exec = parse_str(yaml).unwrap().services["app"]
        .command
        .as_ref()
        .unwrap()
        .to_exec();
    assert_eq!(exec, vec!["node", "server.js"]);
}

#[test]
fn ports_short() {
    let yaml = r#"
services:
  web:
    image: nginx
    ports: ["80:80", "443:443"]
"#;
    assert_eq!(parse_str(yaml).unwrap().services["web"].ports.len(), 2);
}

#[test]
fn volumes_short() {
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
fn networks_list() {
    let yaml = r#"
services:
  web:
    image: nginx
    networks: [frontend]
networks:
  frontend:
    driver: bridge
"#;
    let file = parse_str(yaml).unwrap();
    assert!(file.networks.contains_key("frontend"));
    assert_eq!(file.services["web"].networks.names(), vec!["frontend"]);
}

#[test]
fn healthcheck() {
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
    let hc = parse_str(yaml).unwrap().services["db"]
        .healthcheck
        .as_ref()
        .unwrap()
        .retries;
    assert_eq!(hc, Some(10));
}

#[test]
fn depends_on_list() {
    let yaml = r#"
services:
  app:
    image: node:20
    depends_on: [db, redis]
  db:
    image: postgres:17
  redis:
    image: redis:8
"#;
    let deps = parse_str(yaml).unwrap().services["app"]
        .depends_on
        .service_names();
    assert!(deps.contains(&"db".to_string()));
    assert!(deps.contains(&"redis".to_string()));
}

#[test]
fn depends_on_map_with_condition() {
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
    let condition = file.services["app"].depends_on.condition_for("db");
    assert!(matches!(condition, ServiceCondition::ServiceHealthy));
}

#[test]
fn secrets_top_level() {
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
