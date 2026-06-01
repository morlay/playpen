use std::path::Path;

use crate::config::{self, parser};
use crate::sandbox::{AccessVerdict, Verdict};

pub fn check_path_access(config: &config::Config, cwd: &Path, target: &Path) -> AccessVerdict {
    let uri = format!("file://{}", target.display());
    let rules = config
        .filesystem
        .as_ref()
        .map(|f| parser::parse_filesystem_access(&f.access))
        .unwrap_or_default();
    let verdict = match parser::find_filesystem_rule(&rules, cwd, target) {
        Some(r) => match r.prefix {
            config::RulePrefix::Allow => Verdict::Allowed,
            config::RulePrefix::ReadOnly => Verdict::ReadOnly,
            config::RulePrefix::Deny => Verdict::Denied,
        },
        None => Verdict::Denied,
    };
    AccessVerdict::new(verdict, uri)
}

pub fn check_domain_access(config: &config::Config, domain: &str) -> AccessVerdict {
    let uri = format!("https://{}", domain);
    let rules = config
        .network
        .as_ref()
        .map(|n| parser::parse_network_access(&n.access))
        .unwrap_or_default();
    let verdict = match parser::validate_network_domain(&rules, domain) {
        config::ValidationResult::Allowed => Verdict::Allowed,
        _ => Verdict::Denied,
    };
    AccessVerdict::new(verdict, uri)
}
