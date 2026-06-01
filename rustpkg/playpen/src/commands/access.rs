use std::path::Path;

use playpen_config::AppConfig;
use playpen_sandbox::Verdict;

pub fn ls_access(app: &AppConfig, cwd: &Path, paths: &[String]) {
    for path in paths {
        let clean = path.trim_start_matches("./");
        let target = cwd.join(clean);
        let info = playpen_sandbox::check_path_access(&app.sandbox, cwd, &target);
        println!("{} {}", verdict_label(&info.verdict), target.display());
    }
}

pub fn domain_access(app: &AppConfig, domains: &[String]) {
    for domain in domains {
        let v = playpen_sandbox::check_domain_access(&app.sandbox, domain);
        match v.verdict {
            Verdict::Allowed => println!("ALLOW {}", domain),
            _ => println!("DENY  {}", domain),
        }
    }
}

fn verdict_label(verdict: &Verdict) -> &'static str {
    match verdict {
        Verdict::Allowed => "rw",
        Verdict::ReadOnly => "r-",
        Verdict::Denied => "--",
    }
}
