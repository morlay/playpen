use std::path::PathBuf;
use std::sync::Arc;

use playpen_sandbox::{Sandbox, Verdict};

use crate::fs::{
    EditOption, EditResult, FileEntry, FileSystem, FileSystemError, FindOption, GrepMatch,
    GrepOption, MoveOption, MoveResult, ReadOption, ReadResult, WriteOption, WriteResult,
};

pub struct SandboxFileSystem {
    pub(crate) inner: Arc<dyn FileSystem>,
    pub(crate) sandbox: Arc<dyn Sandbox>,
}

fn verdict_rank(v: &Verdict) -> u8 {
    match v {
        Verdict::Allowed => 2,
        Verdict::ReadOnly => 1,
        Verdict::Denied => 0,
    }
}

impl SandboxFileSystem {
    pub fn new(inner: Arc<dyn FileSystem>, sandbox: Arc<dyn Sandbox>) -> Self {
        Self { inner, sandbox }
    }

    fn check(&self, path: &str, min_verdict: Verdict) -> Result<(), FileSystemError> {
        let uri = format!("file://{}", path);
        match self.sandbox.access(&uri).verdict {
            Verdict::Denied => Err(FileSystemError::Permission(format!("沙箱拒绝: {}", path))),
            actual => {
                if verdict_rank(&actual) >= verdict_rank(&min_verdict) {
                    Ok(())
                } else {
                    Err(FileSystemError::Permission(format!("只读: {}", path)))
                }
            }
        }
    }
}

impl FileSystem for SandboxFileSystem {
    fn working_dir(&self) -> PathBuf {
        self.inner.working_dir()
    }

    fn read(&self, opt: ReadOption) -> anyhow::Result<ReadResult> {
        self.check(&opt.path, Verdict::ReadOnly)?;
        self.inner.read(opt)
    }

    fn edit(&self, opt: EditOption) -> anyhow::Result<EditResult> {
        self.check(&opt.path, Verdict::Allowed)?;
        self.inner.edit(opt)
    }

    fn write(&self, opt: WriteOption) -> anyhow::Result<WriteResult> {
        self.check(&opt.path, Verdict::Allowed)?;
        self.inner.write(opt)
    }

    fn grep(&self, opt: GrepOption) -> anyhow::Result<Box<dyn Iterator<Item = GrepMatch>>> {
        let search = opt.path.as_deref().unwrap_or(".");
        self.check(search, Verdict::ReadOnly)?;
        self.inner.grep(opt)
    }

    fn find(&self, opt: FindOption) -> anyhow::Result<Box<dyn Iterator<Item = FileEntry>>> {
        let search = opt.path.as_deref().unwrap_or(".");
        self.check(search, Verdict::ReadOnly)?;
        self.inner.find(opt)
    }

    fn r#move(&self, opt: MoveOption) -> anyhow::Result<MoveResult> {
        self.check(&opt.old_path, Verdict::Allowed)?;
        if opt.new_path != "/dev/null" {
            self.check(&opt.new_path, Verdict::Allowed)?;
        }
        self.inner.r#move(opt)
    }
}

#[cfg(test)]
#[path = "fs_test.rs"]
mod fs_test;
