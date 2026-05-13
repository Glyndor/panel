use lynx_compose::env_file::load_env_files;
use std::io::Write;

#[test]
fn basic_key_value() {
    let dir = tempfile::tempdir().unwrap();
    let mut f = std::fs::File::create(dir.path().join("app.env")).unwrap();
    writeln!(f, "# comment").unwrap();
    writeln!(f).unwrap();
    writeln!(f, "DB_HOST=localhost").unwrap();
    writeln!(f, "PORT=5432").unwrap();
    writeln!(f, "NOVALUE").unwrap();

    let map = load_env_files(&["app.env".to_string()], dir.path()).unwrap();
    assert_eq!(map["DB_HOST"], "localhost");
    assert_eq!(map["PORT"], "5432");
    assert_eq!(map["NOVALUE"], "");
}

#[test]
fn string_or_list_single() {
    use lynx_compose::compose::types::StringOrList;
    assert_eq!(
        StringOrList::Single("file.env".to_string()).to_list(),
        vec!["file.env"]
    );
}

#[test]
fn string_or_list_many() {
    use lynx_compose::compose::types::StringOrList;
    let sol = StringOrList::List(vec!["a.env".to_string(), "b.env".to_string()]);
    assert_eq!(sol.to_list().len(), 2);
}
