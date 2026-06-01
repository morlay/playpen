use super::*;

#[test]
fn fs_error_display() {
    let err = FileSystemError::NotFound("test.txt".into());
    assert!(format!("{}", err).contains("test.txt"));
}

#[test]
fn fs_error_from_io() {
    let io = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let err = FileSystemError::Io(format!("a.txt: {}", io));
    assert!(format!("{}", err).contains("a.txt"));
}

#[test]
fn read_option_schema() {
    let schema = schemars::schema_for!(ReadOption);
    let json = serde_json::to_string_pretty(&schema).unwrap();
    assert!(json.contains("path"));
    assert!(json.contains("offset"));
    assert!(json.contains("limit"));
}

#[test]
fn edit_option_schema() {
    let schema = schemars::schema_for!(EditOption);
    let json = serde_json::to_string_pretty(&schema).unwrap();
    assert!(json.contains("path"));
    assert!(json.contains("edits"));
}

#[test]
fn write_option_serde() {
    let opt = WriteOption {
        path: "f.rs".into(),
        content: "hello".into(),
    };
    let json = serde_json::to_string(&opt).unwrap();
    let back: WriteOption = serde_json::from_str(&json).unwrap();
    assert_eq!(back.path, "f.rs");
}

#[test]
fn move_option_serde() {
    let opt = MoveOption {
        old_path: "a.rs".into(),
        new_path: "b.rs".into(),
    };
    let json = serde_json::to_string(&opt).unwrap();
    let back: MoveOption = serde_json::from_str(&json).unwrap();
    assert_eq!(back.old_path, "a.rs");
    assert_eq!(back.new_path, "b.rs");
}
