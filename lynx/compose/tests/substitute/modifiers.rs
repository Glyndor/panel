use lynx_compose::substitute::substitute;
use lynx_compose::ComposeError;
use std::collections::HashMap;

fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[test]
fn bare_dollar_sign() {
    assert_eq!(substitute("cost: $5", &vars(&[])).unwrap(), "cost: $5");
}

#[test]
fn double_dollar_escape() {
    assert_eq!(substitute("$$VAR", &vars(&[("VAR", "hello")])).unwrap(), "$VAR");
}

#[test]
fn bare_var_set() {
    assert_eq!(substitute("$FOO bar", &vars(&[("FOO", "hello")])).unwrap(), "hello bar");
}

#[test]
fn bare_var_unset() {
    assert_eq!(substitute("$MISSING", &vars(&[])).unwrap(), "");
}

#[test]
fn braced_var_set() {
    assert_eq!(substitute("${FOO}", &vars(&[("FOO", "world")])).unwrap(), "world");
}

#[test]
fn braced_var_unset() {
    assert_eq!(substitute("${MISSING}", &vars(&[])).unwrap(), "");
}

#[test]
fn default_if_unset_or_empty_nonempty() {
    assert_eq!(substitute("${FOO:-def}", &vars(&[("FOO", "bar")])).unwrap(), "bar");
}

#[test]
fn default_if_unset_or_empty_empty() {
    assert_eq!(substitute("${FOO:-def}", &vars(&[("FOO", "")])).unwrap(), "def");
}

#[test]
fn default_if_unset_or_empty_unset() {
    assert_eq!(substitute("${FOO:-def}", &vars(&[])).unwrap(), "def");
}

#[test]
fn default_if_unset_set_empty() {
    assert_eq!(substitute("${FOO-def}", &vars(&[("FOO", "")])).unwrap(), "");
}

#[test]
fn default_if_unset_unset() {
    assert_eq!(substitute("${FOO-def}", &vars(&[])).unwrap(), "def");
}

#[test]
fn default_if_unset_set_nonempty() {
    assert_eq!(substitute("${FOO-def}", &vars(&[("FOO", "bar")])).unwrap(), "bar");
}

#[test]
fn alt_if_set_and_nonempty() {
    assert_eq!(substitute("${FOO:+alt}", &vars(&[("FOO", "bar")])).unwrap(), "alt");
}

#[test]
fn alt_if_set_empty_value() {
    assert_eq!(substitute("${FOO:+alt}", &vars(&[("FOO", "")])).unwrap(), "");
}

#[test]
fn alt_if_set_unset() {
    assert_eq!(substitute("${FOO:+alt}", &vars(&[])).unwrap(), "");
}

#[test]
fn alt_if_set_counts_empty() {
    assert_eq!(substitute("${FOO+alt}", &vars(&[("FOO", "")])).unwrap(), "alt");
}

#[test]
fn alt_if_set_unset_returns_empty() {
    assert_eq!(substitute("${FOO+alt}", &vars(&[])).unwrap(), "");
}

#[test]
fn error_if_unset_or_empty_nonempty() {
    assert_eq!(substitute("${FOO:?err}", &vars(&[("FOO", "bar")])).unwrap(), "bar");
}

#[test]
fn error_if_unset_or_empty_unset() {
    let result = substitute("${FOO:?err msg}", &vars(&[]));
    assert!(
        matches!(result, Err(ComposeError::RequiredVarNotSet { ref var, ref msg }) if var == "FOO" && msg == "err msg")
    );
}

#[test]
fn error_if_unset_or_empty_empty() {
    assert!(substitute("${FOO:?err msg}", &vars(&[("FOO", "")])).is_err());
}

#[test]
fn error_if_unset_set_empty_ok() {
    assert_eq!(substitute("${FOO?err}", &vars(&[("FOO", "")])).unwrap(), "");
}

#[test]
fn error_if_unset_unset() {
    assert!(substitute("${FOO?err}", &vars(&[])).is_err());
}

#[test]
fn chained() {
    let v = vars(&[("A", "hello"), ("B", "world")]);
    assert_eq!(substitute("$A ${B}", &v).unwrap(), "hello world");
}

#[test]
fn yaml_default_in_string() {
    assert_eq!(
        substitute("image: myapp:${TAG:-latest}", &vars(&[])).unwrap(),
        "image: myapp:latest"
    );
}
