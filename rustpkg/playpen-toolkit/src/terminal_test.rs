use super::*;

#[test]
fn terminal_cmd_schema() {
    let schema = schemars::schema_for!(Command);
    let json = serde_json::to_string_pretty(&schema).unwrap();
    assert!(json.contains("command"));
    assert!(json.contains("cwd"));
}

#[test]
fn terminal_error_display() {
    assert!(format!("{}", ExecError::Exec("fail".into())).contains("fail"));
    assert!(format!("{}", ExecError::Timeout("超时".into())).contains("超时"));
}

#[test]
fn output_type_serde() {
    let stdout = CommandOutput::Stdout {
        text: "hello".into(),
    };
    let json = serde_json::to_string(&stdout).unwrap();
    assert!(json.contains("hello"));
    assert!(json.contains("stdout"));

    let exited = CommandOutput::Exited { code: 0 };
    let json = serde_json::to_string(&exited).unwrap();
    assert!(json.contains("exited"));
}

#[test]
fn spawn_failed_serde() {
    let sf = CommandOutput::SpawnFailed {
        message: "sh not found".into(),
    };
    let json = serde_json::to_string(&sf).unwrap();
    assert!(json.contains("spawn_failed"));
    assert!(json.contains("sh not found"));

    let deserialized: CommandOutput = serde_json::from_str(&json).unwrap();
    assert!(matches!(deserialized, CommandOutput::SpawnFailed { message } if message == "sh not found"));
}
