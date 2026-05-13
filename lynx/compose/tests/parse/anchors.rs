use lynx_compose::parse_str;

#[test]
fn yaml_anchor_and_alias() {
    let yaml = r#"
x-common: &common
  image: alpine
  restart: always
  environment:
    LOG_LEVEL: info

services:
  web:
    <<: *common
  api:
    <<: *common
    image: node:20
"#;
    let file = parse_str(yaml).unwrap();

    // web inherits image and environment from anchor.
    assert_eq!(file.services["web"].image.as_deref(), Some("alpine"));
    assert!(file.services["web"]
        .environment
        .to_map()
        .contains_key("LOG_LEVEL"));

    // api overrides image, keeps environment.
    assert_eq!(file.services["api"].image.as_deref(), Some("node:20"));
    assert!(file.services["api"]
        .environment
        .to_map()
        .contains_key("LOG_LEVEL"));
}

#[test]
fn yaml_anchor_passthrough_for_environment() {
    let yaml = r#"
x-env: &env
  NODE_ENV: production
  PORT: "3000"

services:
  app:
    image: node
    environment: *env
"#;
    let file = parse_str(yaml).unwrap();
    let env = file.services["app"].environment.to_map();
    assert_eq!(env.get("NODE_ENV").and_then(|v| v.clone()).as_deref(), Some("production"));
    assert_eq!(env.get("PORT").and_then(|v| v.clone()).as_deref(), Some("3000"));
}
