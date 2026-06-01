pub(crate) mod parser;
pub(crate) mod policy;
pub(crate) mod shell;

pub use parser::{
    find_filesystem_rule, is_path_pattern, parse_filesystem_access, parse_filesystem_rules,
    parse_filesystem_string, parse_network_access, parse_network_rules, parse_network_string,
    resolve_pattern, simple_glob_match, validate_filesystem_path, validate_network_domain,
};
pub use policy::PolicyClassification;

use merge::Merge;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default, Clone, Merge)]
pub struct Config {
    #[merge(strategy = merge::option::recurse)]
    pub network: Option<AllowSection>,
    #[merge(strategy = merge::option::recurse)]
    pub filesystem: Option<AllowSection>,
    #[merge(strategy = merge::option::recurse)]
    pub shell: Option<ShellSection>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, Merge)]
pub struct AllowSection {
    #[serde(default)]
    #[merge(strategy = merge::vec::append)]
    pub access: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, Merge)]
pub struct ShellSection {
    #[merge(strategy = merge::option::overwrite_none)]
    pub allow_pipe: Option<bool>,
    #[merge(strategy = merge::option::overwrite_none)]
    pub allow_multiple: Option<bool>,
    #[serde(default)]
    #[merge(strategy = merge::vec::append)]
    pub allow: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum RulePrefix {
    Allow,
    Deny,
    ReadOnly,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ParsedRule {
    pub raw: String,
    pub prefix: RulePrefix,
    pub pattern: String,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ValidationResult {
    Allowed,
    Denied,
    ReadOnly,
}
