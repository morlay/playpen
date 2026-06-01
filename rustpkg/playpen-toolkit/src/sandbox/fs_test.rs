use super::*;
use crate::fs::{
    EditOp, EditOption, FileSystem, FileSystemError, FindOption, GrepOption, MoveOption,
    ReadOption, WriteOption,
};
use crate::native::NativeFileSystem;
use std::sync::Arc;

struct DenyAll;
impl playpen_sandbox::Sandbox for DenyAll {
    fn access(&self, uri: &str) -> playpen_sandbox::AccessVerdict {
        playpen_sandbox::AccessVerdict::denied(uri)
    }
    fn wrap_command(
        &self,
        _: playpen_sandbox::Command,
    ) -> Result<playpen_sandbox::Command, playpen_sandbox::Error> {
        unreachable!()
    }
}

struct ReadOnlyAccess;
impl playpen_sandbox::Sandbox for ReadOnlyAccess {
    fn access(&self, uri: &str) -> playpen_sandbox::AccessVerdict {
        playpen_sandbox::AccessVerdict::readonly(uri)
    }
    fn wrap_command(
        &self,
        _: playpen_sandbox::Command,
    ) -> Result<playpen_sandbox::Command, playpen_sandbox::Error> {
        unreachable!()
    }
}

struct AllowAll;
impl playpen_sandbox::Sandbox for AllowAll {
    fn access(&self, uri: &str) -> playpen_sandbox::AccessVerdict {
        playpen_sandbox::AccessVerdict::allowed(uri)
    }
    fn wrap_command(
        &self,
        cmd: playpen_sandbox::Command,
    ) -> Result<playpen_sandbox::Command, playpen_sandbox::Error> {
        Ok(cmd)
    }
}

fn test_dir() -> tempfile::TempDir {
    tempfile::tempdir_in("/tmp").unwrap()
}

fn make_fs(dir: &tempfile::TempDir) -> SandboxFileSystem {
    SandboxFileSystem {
        inner: Arc::new(NativeFileSystem {
            working_dir: dir.path().to_path_buf(),
        }),
        sandbox: Arc::new(AllowAll),
    }
}

#[test]
fn read_through_sandbox() {
    let dir = test_dir();
    std::fs::write(dir.path().join("f.txt"), "hello").unwrap();
    let result = make_fs(&dir)
        .read(ReadOption {
            path: "f.txt".into(),
            offset: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(result.content, "hello");
}

#[test]
fn denied_by_sandbox() {
    let dir = test_dir();
    std::fs::write(dir.path().join("f.txt"), "hello").unwrap();
    let fs = SandboxFileSystem {
        inner: Arc::new(NativeFileSystem {
            working_dir: dir.path().to_path_buf(),
        }),
        sandbox: Arc::new(DenyAll),
    };
    let err = fs
        .read(ReadOption {
            path: "f.txt".into(),
            offset: None,
            limit: None,
        })
        .unwrap_err();
    assert!(
        err.downcast_ref::<FileSystemError>()
            .is_some_and(|e| matches!(e, FileSystemError::Permission(_)))
    );
}

#[test]
fn readonly_allows_read() {
    let dir = test_dir();
    std::fs::write(dir.path().join("f.txt"), "hello").unwrap();
    let fs = SandboxFileSystem {
        inner: Arc::new(NativeFileSystem {
            working_dir: dir.path().to_path_buf(),
        }),
        sandbox: Arc::new(ReadOnlyAccess),
    };
    let result = fs
        .read(ReadOption {
            path: "f.txt".into(),
            offset: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(result.content, "hello");
}

#[test]
fn readonly_allows_grep() {
    let dir = test_dir();
    std::fs::write(dir.path().join("f.txt"), "hello world").unwrap();
    let fs = SandboxFileSystem {
        inner: Arc::new(NativeFileSystem {
            working_dir: dir.path().to_path_buf(),
        }),
        sandbox: Arc::new(ReadOnlyAccess),
    };
    let results: Vec<_> = fs
        .grep(GrepOption {
            pattern: "hello".into(),
            path: Some(".".into()),
            glob: None,
            ignore_case: None,
        })
        .unwrap()
        .collect();
    assert!(!results.is_empty());
}

#[test]
fn readonly_allows_find() {
    let dir = test_dir();
    std::fs::write(dir.path().join("f.txt"), "hello").unwrap();
    let fs = SandboxFileSystem {
        inner: Arc::new(NativeFileSystem {
            working_dir: dir.path().to_path_buf(),
        }),
        sandbox: Arc::new(ReadOnlyAccess),
    };
    let results: Vec<_> = fs
        .find(FindOption {
            pattern: "*.txt".into(),
            path: Some(".".into()),
            limit: None,
        })
        .unwrap()
        .collect();
    assert!(!results.is_empty());
}

#[test]
fn readonly_denies_write() {
    let dir = test_dir();
    let fs = SandboxFileSystem {
        inner: Arc::new(NativeFileSystem {
            working_dir: dir.path().to_path_buf(),
        }),
        sandbox: Arc::new(ReadOnlyAccess),
    };
    let err = fs
        .write(WriteOption {
            path: "f.txt".into(),
            content: "data".into(),
        })
        .unwrap_err();
    assert!(
        err.downcast_ref::<FileSystemError>()
            .is_some_and(|e| matches!(e, FileSystemError::Permission(_)))
    );
}

#[test]
fn readonly_denies_edit() {
    let dir = test_dir();
    let fs = SandboxFileSystem {
        inner: Arc::new(NativeFileSystem {
            working_dir: dir.path().to_path_buf(),
        }),
        sandbox: Arc::new(ReadOnlyAccess),
    };
    let err = fs
        .edit(EditOption {
            path: "f.txt".into(),
            edits: vec![EditOp {
                old_text: "a".into(),
                new_text: "b".into(),
            }],
        })
        .unwrap_err();
    assert!(
        err.downcast_ref::<FileSystemError>()
            .is_some_and(|e| matches!(e, FileSystemError::Permission(_)))
    );
}

#[test]
fn readonly_denies_move() {
    let dir = test_dir();
    std::fs::write(dir.path().join("f.txt"), "hello").unwrap();
    let fs = SandboxFileSystem {
        inner: Arc::new(NativeFileSystem {
            working_dir: dir.path().to_path_buf(),
        }),
        sandbox: Arc::new(ReadOnlyAccess),
    };
    let err = fs
        .r#move(MoveOption {
            old_path: "f.txt".into(),
            new_path: "g.txt".into(),
        })
        .unwrap_err();
    assert!(
        err.downcast_ref::<FileSystemError>()
            .is_some_and(|e| matches!(e, FileSystemError::Permission(_)))
    );
}
