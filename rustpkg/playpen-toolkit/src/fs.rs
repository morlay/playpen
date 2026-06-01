use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReadOption {
    pub path: String,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EditOp {
    pub old_text: String,
    pub new_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EditOption {
    pub path: String,
    pub edits: Vec<EditOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WriteOption {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GrepOption {
    pub pattern: String,
    pub path: Option<String>,
    pub glob: Option<String>,
    pub ignore_case: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FindOption {
    pub pattern: String,
    pub path: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MoveOption {
    pub old_path: String,
    pub new_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResult {
    pub r#type: Option<String>,
    pub content: String,
    pub chunked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditResult {
    pub path: String,
    pub ops: Vec<EditOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteResult {
    pub path: String,
    pub old_text: Option<String>,
    pub new_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepMatch {
    pub path: String,
    pub contents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveResult {
    pub deleted: Option<bool>,
}

#[derive(Debug, thiserror::Error)]
pub enum FileSystemError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("is a directory: {0}")]
    IsDir(String),
    #[error("not a directory: {0}")]
    NotDir(String),
    #[error("permission denied: {0}")]
    Permission(String),
    #[error("invalid pattern: {0}")]
    InvalidPattern(String),
    #[error("io error: {0}")]
    Io(String),
}

pub trait FileSystem: Send + Sync {
    fn working_dir(&self) -> PathBuf;

    fn read(&self, opt: ReadOption) -> anyhow::Result<ReadResult>;
    fn edit(&self, opt: EditOption) -> anyhow::Result<EditResult>;
    fn write(&self, opt: WriteOption) -> anyhow::Result<WriteResult>;
    fn grep(&self, opt: GrepOption) -> anyhow::Result<Box<dyn Iterator<Item = GrepMatch>>>;
    fn find(&self, opt: FindOption) -> anyhow::Result<Box<dyn Iterator<Item = FileEntry>>>;
    fn r#move(&self, opt: MoveOption) -> anyhow::Result<MoveResult>;
}

#[cfg(test)]
#[path = "fs_test.rs"]
mod tests;
