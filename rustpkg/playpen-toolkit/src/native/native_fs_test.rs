use super::*;
use crate::fs::{EditOp, FileSystem, MoveOption, ReadOption, WriteOption};

#[test]
fn read_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.txt");
    std::fs::write(&path, "line1\nline2\nline3\n").unwrap();

    let fs = NativeFileSystem {
        working_dir: dir.path().to_path_buf(),
    };
    let result = fs
        .read(ReadOption {
            path: "test.txt".into(),
            offset: None,
            limit: None,
        })
        .unwrap();
    assert!(result.content.contains("line1"));
}

#[test]
fn read_with_offset_limit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.txt");
    let content: String = (0..200).map(|i| format!("line{}\n", i)).collect();
    std::fs::write(&path, &content).unwrap();

    let fs = NativeFileSystem {
        working_dir: dir.path().to_path_buf(),
    };
    let result = fs
        .read(ReadOption {
            path: "test.txt".into(),
            offset: Some(10),
            limit: Some(5),
        })
        .unwrap();
    assert!(result.chunked);
    assert!(result.content.contains("line9"));
}

#[test]
fn read_with_offset_no_limit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.txt");
    let content: String = (0..10).map(|i| format!("line{}\n", i)).collect();
    std::fs::write(&path, &content).unwrap();

    let fs = NativeFileSystem {
        working_dir: dir.path().to_path_buf(),
    };
    // offset > 0, limit = None: 验证不会因 usize::MAX 溢出而 panic
    let result = fs
        .read(ReadOption {
            path: "test.txt".into(),
            offset: Some(5),
            limit: None,
        })
        .unwrap();
    assert!(result.content.contains("line4")); // 第5行，0-indexed = 4
    assert!(result.content.contains("line9")); // 最后一行
}

#[test]
fn write_and_read() {
    let dir = tempfile::tempdir().unwrap();
    let fs = NativeFileSystem {
        working_dir: dir.path().to_path_buf(),
    };
    fs.write(WriteOption {
        path: "new.txt".into(),
        content: "hello".into(),
    })
    .unwrap();
    let result = fs
        .read(ReadOption {
            path: "new.txt".into(),
            offset: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(result.content, "hello");
}

#[test]
fn edit_unique_match() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("f.txt"), "old").unwrap();
    let fs = NativeFileSystem {
        working_dir: dir.path().to_path_buf(),
    };
    let op = EditOp {
        old_text: "old".into(),
        new_text: "new".into(),
    };
    fs.edit(EditOption {
        path: "f.txt".into(),
        edits: vec![op],
    })
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("f.txt")).unwrap(),
        "new"
    );
}

#[test]
fn grep_with_glob_does_not_skip_directories() {
    let dir = tempfile::tempdir().unwrap();
    // 创建目录结构:
    //   tmp/
    //     subdir/
    //       hello.rs
    //       hello.txt
    //     other.txt
    std::fs::create_dir(dir.path().join("subdir")).unwrap();
    std::fs::write(dir.path().join("subdir/hello.rs"), "fn hello() {}\n").unwrap();
    std::fs::write(dir.path().join("subdir/hello.txt"), "hello\n").unwrap();
    std::fs::write(dir.path().join("other.txt"), "other\n").unwrap();

    let fs = NativeFileSystem {
        working_dir: dir.path().to_path_buf(),
    };
    // glob = "*.rs" 应该只匹配 .rs 文件，且不应因目录名不匹配而跳过子目录
    let results: Vec<_> = fs
        .grep(GrepOption {
            pattern: "hello".into(),
            path: None,
            glob: Some("*.rs".into()),
            ignore_case: None,
        })
        .unwrap()
        .collect();
    assert_eq!(results.len(), 1, "应只找到 subdir/hello.rs");
    assert!(
        results[0].path.contains("hello.rs"),
        "路径应包含 hello.rs，实际: {}",
        results[0].path
    );
}

#[test]
fn move_and_delete() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "data").unwrap();
    let fs = NativeFileSystem {
        working_dir: dir.path().to_path_buf(),
    };
    fs.r#move(MoveOption {
        old_path: "a.txt".into(),
        new_path: "/dev/null".into(),
    })
    .unwrap();
    assert!(!dir.path().join("a.txt").exists());
}
