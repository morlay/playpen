use std::path::{Path, PathBuf};

use crate::fs::{
    EditOption, EditResult, FileEntry, FileSystem, FileSystemError, FindOption, GrepMatch,
    GrepOption, MoveOption, MoveResult, ReadOption, ReadResult, WriteOption, WriteResult,
};

pub struct NativeFileSystem {
    pub(crate) working_dir: PathBuf,
}

impl NativeFileSystem {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }
    fn resolve(&self, path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_relative() {
            self.working_dir.join(p)
        } else {
            p.to_path_buf()
        }
    }
}

impl FileSystem for NativeFileSystem {
    fn working_dir(&self) -> PathBuf {
        self.working_dir.clone()
    }

    fn read(&self, opt: ReadOption) -> anyhow::Result<ReadResult> {
        let target = self.resolve(&opt.path);
        if target.is_dir() {
            return Err(FileSystemError::IsDir(opt.path).into());
        }
        if !target.is_file() {
            return Err(FileSystemError::NotFound(opt.path).into());
        }
        let content = std::fs::read_to_string(&target)
            .map_err(|e| FileSystemError::Io(format!("{}: {}", opt.path.clone(), e)))?;
        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();
        let start = opt.offset.unwrap_or(1).saturating_sub(1);
        let limit = opt.limit.unwrap_or(usize::MAX);
        let end = start.saturating_add(limit).min(total);
        if start >= total {
            return Ok(ReadResult {
                r#type: None,
                content: String::new(),
                chunked: false,
            });
        }
        let selected: Vec<&str> = lines[start..end].to_vec();
        let chunked = opt.offset.is_some() || opt.limit.is_some() || total > 100;
        let mut result = String::new();
        if chunked {
            for (i, line) in selected.iter().enumerate() {
                result.push_str(&format!("{:>6}\t{}\n", start + i + 1, line));
            }
        } else {
            result = selected.join("\n");
        }
        if end < total {
            result.push_str(&format!(
                "\n... （共 {} 行，显示 {}–{}）",
                total,
                start + 1,
                end
            ));
        }
        Ok(ReadResult {
            r#type: None,
            content: result,
            chunked,
        })
    }

    fn edit(&self, opt: EditOption) -> anyhow::Result<EditResult> {
        let target = self.resolve(&opt.path);
        let mut content = std::fs::read_to_string(&target)
            .map_err(|e| FileSystemError::Io(format!("{}: {}", opt.path.clone(), e)))?;
        for (i, op) in opt.edits.iter().enumerate() {
            let count = content.matches(&op.old_text).count();
            let truncated: String = op.old_text.chars().take(10).collect();
            if count == 0 {
                return Err(FileSystemError::Io(format!(
                    "第 {} 个编辑操作：未找到匹配:\n{}",
                    i + 1,
                    truncated
                ))
                .into());
            }
            if count > 1 {
                return Err(FileSystemError::Io(format!(
                    "第 {} 个编辑操作：匹配 {} 次（需唯一）:\n{}",
                    i + 1,
                    count,
                    truncated
                ))
                .into());
            }
            content = content.replacen(&op.old_text, &op.new_text, 1);
        }
        std::fs::write(&target, &content)
            .map_err(|e| FileSystemError::Io(format!("{}: {}", opt.path.clone(), e)))?;
        Ok(EditResult {
            path: opt.path,
            ops: opt.edits,
        })
    }

    fn write(&self, opt: WriteOption) -> anyhow::Result<WriteResult> {
        let target = self.resolve(&opt.path);
        let old_text = if target.exists() {
            std::fs::read_to_string(&target).ok()
        } else {
            None
        };
        if let Some(parent) = target.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .map_err(|e| FileSystemError::Io(format!("{}: {}", parent.display(), e)))?;
        }
        std::fs::write(&target, &opt.content)
            .map_err(|e| FileSystemError::Io(format!("{}: {}", opt.path.clone(), e)))?;
        Ok(WriteResult {
            path: opt.path,
            old_text,
            new_text: opt.content,
        })
    }

    fn grep(&self, opt: GrepOption) -> anyhow::Result<Box<dyn Iterator<Item = GrepMatch>>> {
        use grep::regex::RegexMatcherBuilder;
        use grep::searcher::{SearcherBuilder, Sink, SinkMatch};
        let search_path = self.resolve(opt.path.as_deref().unwrap_or("."));
        if !search_path.is_dir() {
            return Err(FileSystemError::NotDir(search_path.display().to_string()).into());
        }
        let mut builder = RegexMatcherBuilder::new();
        if opt.ignore_case.unwrap_or(false) {
            builder.case_insensitive(true);
        }
        let matcher = builder
            .build(&opt.pattern)
            .map_err(|e| FileSystemError::InvalidPattern(format!("{}: {}", opt.pattern, e)))?;
        let mut files = walk_files(&search_path, opt.glob.as_deref())?;
        let mut results: Vec<GrepMatch> = Vec::new();
        let mut searcher = SearcherBuilder::new().build();
        for file_path in files.drain(..) {
            struct Collector {
                lines: Vec<String>,
            }
            impl Sink for Collector {
                type Error = std::io::Error;
                fn matched(
                    &mut self,
                    _: &grep::searcher::Searcher,
                    mat: &SinkMatch<'_>,
                ) -> Result<bool, Self::Error> {
                    let line = std::str::from_utf8(mat.bytes())
                        .unwrap_or("")
                        .trim_end()
                        .to_string();
                    self.lines.push(line);
                    Ok(true)
                }
            }
            let mut c = Collector { lines: Vec::new() };
            let content = std::fs::read_to_string(&file_path)
                .map_err(|e| FileSystemError::Io(format!("{}: {}", file_path.display(), e)))?;
            searcher
                .search_slice(&matcher, content.as_bytes(), &mut c)
                .map_err(|e| FileSystemError::Io(e.to_string()))?;
            if !c.lines.is_empty() {
                let display = file_path
                    .strip_prefix(&self.working_dir)
                    .unwrap_or(&file_path)
                    .display()
                    .to_string();
                results.push(GrepMatch {
                    path: display,
                    contents: c.lines,
                });
            }
        }
        Ok(Box::new(results.into_iter()))
    }

    fn find(&self, opt: FindOption) -> anyhow::Result<Box<dyn Iterator<Item = FileEntry>>> {
        let search_path = self.resolve(opt.path.as_deref().unwrap_or("."));
        if !search_path.is_dir() {
            return Err(FileSystemError::NotDir(search_path.display().to_string()).into());
        }
        let pattern = glob::Pattern::new(&opt.pattern)
            .map_err(|e| FileSystemError::InvalidPattern(format!("{}: {}", opt.pattern, e)))?;
        let files = walk_files(&search_path, None)?;
        let limit = opt.limit.unwrap_or(100);
        let mut results: Vec<FileEntry> = Vec::new();
        for fp in files {
            let name = fp.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let rel = fp
                .strip_prefix(&self.working_dir)
                .unwrap_or(&fp)
                .to_string_lossy()
                .to_string();
            if pattern.matches(name) || pattern.matches(&rel) {
                results.push(FileEntry { path: rel });
                if results.len() >= limit {
                    break;
                }
            }
        }
        Ok(Box::new(results.into_iter()))
    }

    fn r#move(&self, opt: MoveOption) -> anyhow::Result<MoveResult> {
        let src = self.resolve(&opt.old_path);
        if !src.exists() {
            return Err(FileSystemError::NotFound(opt.old_path).into());
        }
        if opt.new_path == "/dev/null" {
            if src.is_dir() {
                std::fs::remove_dir_all(&src)
                    .map_err(|e| FileSystemError::Io(format!("{}: {}", opt.old_path.clone(), e)))?;
            } else {
                std::fs::remove_file(&src)
                    .map_err(|e| FileSystemError::Io(format!("{}: {}", opt.old_path.clone(), e)))?;
            }
            return Ok(MoveResult {
                deleted: Some(true),
            });
        }
        let dst = self.resolve(&opt.new_path);
        if let Some(parent) = dst.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .map_err(|e| FileSystemError::Io(format!("{}: {}", parent.display(), e)))?;
        }
        std::fs::rename(&src, &dst).map_err(|e| {
            FileSystemError::Io(format!("{} -> {}: {}", opt.old_path, opt.new_path, e))
        })?;
        Ok(MoveResult { deleted: None })
    }
}

fn walk_files(
    search_path: &Path,
    glob_filter: Option<&str>,
) -> Result<Vec<PathBuf>, FileSystemError> {
    use ignore::WalkBuilder;
    let mut walk = WalkBuilder::new(search_path);
    walk.standard_filters(true);
    if let Some(glob_str) = glob_filter {
        let g = glob::Pattern::new(glob_str)
            .map_err(|e| FileSystemError::InvalidPattern(format!("glob: {}", e)))?;
        walk.filter_entry(move |e| {
            // 目录始终允许进入，否则 glob 会过滤掉目录导致子树被跳过
            if e.file_type().is_some_and(|ft| ft.is_dir()) {
                return true;
            }
            let name = e.file_name().to_str().unwrap_or("");
            g.matches(name) || g.matches(e.path().to_string_lossy().as_ref())
        });
    }
    let mut files = Vec::new();
    for entry in walk.build() {
        let entry = entry.map_err(|e| FileSystemError::Io(e.to_string()))?;
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        files.push(entry.into_path());
    }
    Ok(files)
}

#[cfg(test)]
#[path = "native_fs_test.rs"]
mod tests;
