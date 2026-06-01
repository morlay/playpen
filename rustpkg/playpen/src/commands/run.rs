use std::path::Path;
use std::process;

use playpen_config::AppConfig;
use playpen_sandbox::Command;

pub fn run_command(command: &str, cwd: &Path, app: &AppConfig) {
    let sb = playpen_sandbox::create(&app.sandbox, cwd);
    let req = Command::new(command).with_cwd(cwd.to_path_buf());
    match sb.wrap_command(req) {
        Ok(approved) => {
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(&approved.command)
                .current_dir(approved.cwd.unwrap_or_else(|| cwd.to_path_buf()))
                .status();
            match output {
                Ok(s) => process::exit(s.code().unwrap_or(1)),
                Err(e) => {
                    eprintln!("playpen: {}", e);
                    process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("playpen: {}", e);
            process::exit(1);
        }
    }
}

pub fn interactive_mode(cwd: &Path, app: &AppConfig) {
    use rustyline::DefaultEditor;
    use rustyline::error::ReadlineError;

    let sb = playpen_sandbox::create(&app.sandbox, cwd);

    let mut rl = match DefaultEditor::new() {
        Ok(editor) => editor,
        Err(e) => {
            eprintln!("playpen: {}", e);
            process::exit(1);
        }
    };

    loop {
        match rl.readline("$ ") {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed == "exit" || trimmed == "quit" {
                    break;
                }
                let _ = rl.add_history_entry(&line);
                let req = Command::new(trimmed).with_cwd(cwd.to_path_buf());
                match sb.wrap_command(req) {
                    Ok(approved) => {
                        let output = std::process::Command::new("sh")
                            .arg("-c")
                            .arg(&approved.command)
                            .current_dir(approved.cwd.unwrap_or_else(|| cwd.to_path_buf()))
                            .status();
                        match output {
                            Ok(s) if s.code() != Some(0) => {
                                eprintln!("playpen: 命令退出码 {}", s.code().unwrap_or(-1))
                            }
                            Err(e) => eprintln!("playpen: {}", e),
                            _ => {}
                        }
                    }
                    Err(e) => eprintln!("playpen: {}", e),
                }
            }
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => break,
            Err(err) => {
                eprintln!("playpen: {}", err);
                break;
            }
        }
    }
}
