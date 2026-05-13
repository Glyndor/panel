use lynx_compose::{parse_str, resolve_order};

#[test]
fn no_deps() {
    let yaml = "services:\n  a:\n    image: alpine\n  b:\n    image: alpine\n  c:\n    image: alpine\n";
    assert_eq!(resolve_order(&parse_str(yaml).unwrap()).unwrap().len(), 3);
}

#[test]
fn linear_chain() {
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
    let order = resolve_order(&parse_str(yaml).unwrap()).unwrap();
    let pos = |s: &str| order.iter().position(|x| x == s).unwrap();
    assert!(pos("a") < pos("b"));
    assert!(pos("b") < pos("c"));
}

#[test]
fn diamond() {
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
    let order = resolve_order(&parse_str(yaml).unwrap()).unwrap();
    let pos = |s: &str| order.iter().position(|x| x == s).unwrap();
    assert!(pos("db") < pos("api"));
    assert!(pos("db") < pos("worker"));
    assert!(pos("api") < pos("app"));
    assert!(pos("worker") < pos("app"));
}

#[test]
fn circular_dependency_error() {
    let yaml = r#"
services:
  a:
    image: alpine
    depends_on: [b]
  b:
    image: alpine
    depends_on: [a]
"#;
    assert!(resolve_order(&parse_str(yaml).unwrap()).is_err());
}

#[test]
fn missing_dependency_error() {
    let yaml = "services:\n  a:\n    image: alpine\n    depends_on: [nonexistent]\n";
    assert!(resolve_order(&parse_str(yaml).unwrap()).is_err());
}
