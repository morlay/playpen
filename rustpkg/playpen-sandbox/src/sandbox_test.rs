use super::*;
use std::collections::HashMap;

struct MockSandbox;

impl Sandbox for MockSandbox {
    fn access(&self, uri: &str) -> AccessVerdict {
        match uri {
            "file:///allowed" => AccessVerdict::allowed(uri),
            _ => AccessVerdict::denied(uri),
        }
    }

    fn wrap_command(&self, cmd: Command) -> Result<Command, Error> {
        if cmd.command == "forbidden" {
            Err(Error::Forbidden("禁止".into()))
        } else {
            Ok(cmd)
        }
    }
}

#[test]
fn access_allowed() {
    let s = MockSandbox;
    let v = s.access("file:///allowed");
    assert!(matches!(v.verdict, Verdict::Allowed));
    assert_eq!(v.url, "file:///allowed");
}

#[test]
fn access_denied() {
    let s = MockSandbox;
    let v = s.access("file:///unknown");
    assert!(matches!(v.verdict, Verdict::Denied));
    assert_eq!(v.url, "file:///unknown");
}

#[test]
fn wrap_command_ok() {
    let s = MockSandbox;
    let cmd = Command {
        command: "echo hello".into(),
        cwd: None,
        env: HashMap::new(),
    };
    let result = s.wrap_command(cmd).unwrap();
    assert_eq!(result.command, "echo hello");
}

#[test]
fn wrap_command_forbidden() {
    let s = MockSandbox;
    let cmd = Command {
        command: "forbidden".into(),
        cwd: None,
        env: HashMap::new(),
    };
    assert!(s.wrap_command(cmd).is_err());
}
