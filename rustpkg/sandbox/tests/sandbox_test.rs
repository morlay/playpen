use sandbox::Sandbox;
use std::path::PathBuf;

macro_rules! skip_if_sandboxed {
    () => {
        if std::env::var("PLAYPEN_SANDBOXED").is_ok() { return; }
    };
}

fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn shell_allow_cat() -> sandbox::config::Config {
    sandbox::config::Config {
        shell: Some(sandbox::config::ShellSection {
            allow_pipe: Some(true),
            allow_multiple: Some(true),
            allow: r#"cat *
echo *"#
                .lines()
                .map(|s| s.to_string())
                .collect(),
        }),
        ..Default::default()
    }
}

fn tmpdir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test_tmp");
    std::fs::create_dir_all(&base).unwrap();
    let dir = base.join(format!("playpen_test_{}", id));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn exec_no_rules_allowed() {
    skip_if_sandboxed!();
    let config = Sandbox::create_config(&Default::default(), &cwd(), "bash");
    let result = Sandbox::exec("echo hello", &cwd(), &config);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().code, 0);
}

#[test]
fn exec_command_not_found() {
    skip_if_sandboxed!();
    let config = Sandbox::create_config(&Default::default(), &cwd(), "bash");
    let result = Sandbox::exec("nonexistent_command_xyz", &cwd(), &config);
    assert_eq!(result.unwrap().code, 127);
}

#[test]
fn exec_deny_command_blocked() {
    skip_if_sandboxed!();
    let config = Sandbox::create_config(&shell_allow_cat(), &cwd(), "bash");
    let result = Sandbox::exec("rm -f /tmp/nonexistent", &cwd(), &config);
    assert!(result.is_err());
}

fn access_lines(raw: &str) -> Vec<String> {
    raw.lines().map(|s| s.to_string()).collect()
}

#[test]
fn exec_deny_read() {
    skip_if_sandboxed!();
    let dir = tmpdir();
    let file = dir.join(".env");
    std::fs::write(&file, "secret").unwrap();

    let config = Sandbox::create_config(
        &sandbox::config::Config {
            filesystem: Some(sandbox::config::AllowSection {
                access: access_lines("rw .\n-- .env"),
            }),
            ..shell_allow_cat()
        },
        &dir,
        "bash",
    );

    let cmd = format!("cat {}", file.display());
    let result = Sandbox::exec(&cmd, &dir, &config);
    assert!(result.is_ok());
    assert_ne!(result.unwrap().code, 0, "deny 读应被 seatbelt 拒绝（exit != 0）");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exec_deny_write() {
    skip_if_sandboxed!();
    let dir = tmpdir();
    let file = dir.join(".env");

    let config = Sandbox::create_config(
        &sandbox::config::Config {
            filesystem: Some(sandbox::config::AllowSection {
                access: access_lines("rw .\n-- .env"),
            }),
            ..shell_allow_cat()
        },
        &dir,
        "bash",
    );

    let cmd = format!("echo data > {}", file.display());
    let result = Sandbox::exec(&cmd, &dir, &config);
    assert!(result.is_ok());
    assert_ne!(result.unwrap().code, 0, "deny 写应被 seatbelt 拒绝（exit != 0）");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exec_readonly_write_blocked() {
    skip_if_sandboxed!();
    let dir = tmpdir();
    let file = dir.join("readonly.txt");
    std::fs::write(&file, "data").unwrap();

    let config = Sandbox::create_config(
        &sandbox::config::Config {
            filesystem: Some(sandbox::config::AllowSection {
                access: access_lines("rw .\nr- readonly.txt"),
            }),
            ..shell_allow_cat()
        },
        &dir,
        "bash",
    );

    let result = Sandbox::exec(&format!("cat {}", file.display()), &dir, &config);
    assert_eq!(result.unwrap().code, 0, "只读文件应可读");

    let result = Sandbox::exec(&format!("echo x > {}", file.display()), &dir, &config);
    assert!(result.is_ok());
    assert_ne!(result.unwrap().code, 0, "只读文件写应被拒绝");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exec_readonly_overrides_deny_pattern() {
    skip_if_sandboxed!();
    let dir = tmpdir();
    let pem = dir.join("cert.pem");
    let txt = dir.join("readme.txt");
    std::fs::write(&pem, "cert-data").unwrap();
    std::fs::write(&txt, "text-data").unwrap();

    let config = Sandbox::create_config(
        &sandbox::config::Config {
            filesystem: Some(sandbox::config::AllowSection {
                access: access_lines(&format!("rw .\n-- *.pem\nr- {}", pem.display())),
            }),
            ..shell_allow_cat()
        },
        &dir,
        "bash",
    );

    let result = Sandbox::exec(&format!("cat {}", pem.display()), &dir, &config);
    assert_eq!(result.unwrap().code, 0, "r- 覆盖 -- *.pem 允许读取");

    let result = Sandbox::exec(&format!("cat {}", txt.display()), &dir, &config);
    assert_eq!(result.unwrap().code, 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exec_allow_read_write() {
    skip_if_sandboxed!();
    let dir = tmpdir();
    let file = dir.join("allowed.txt");

    let config = Sandbox::create_config(
        &sandbox::config::Config {
            filesystem: Some(sandbox::config::AllowSection {
                access: vec!["rw .".into()],
            }),
            ..shell_allow_cat()
        },
        &dir,
        "bash",
    );

    let cmd = format!("echo data > {} && cat {}", file.display(), file.display());
    let result = Sandbox::exec(&cmd, &dir, &config);
    assert_eq!(result.unwrap().code, 0, "允许的文件应可读写");

    let _ = std::fs::remove_dir_all(&dir);
}
