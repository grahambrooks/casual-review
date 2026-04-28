use git2::{Delta, DiffFindOptions, DiffOptions, Repository};
use std::cell::RefCell;
use std::ops::Range;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Clone, Debug)]
pub enum DiffSpec {
    WorkingTree,
    Staged,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Modified,
    Renamed,
    Deleted,
    Untracked,
}

#[derive(Clone, Debug)]
pub struct ChangedFile {
    pub path: PathBuf,
    pub status: FileStatus,
    pub new_content: Option<Vec<u8>>,
    pub old_content: Option<Vec<u8>>,
    pub changed_line_ranges: Vec<Range<u32>>,
}

#[derive(Debug, Error)]
pub enum GitError {
    #[error("git error: {0}")]
    Git(#[from] git2::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn changed_files(repo_root: &Path, spec: &DiffSpec) -> Result<Vec<ChangedFile>, GitError> {
    let repo = Repository::discover(repo_root)?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| git2::Error::from_str("bare repository has no working tree"))?
        .to_path_buf();

    let head_tree = match repo.head() {
        Ok(h) => Some(h.peel_to_tree()?),
        Err(_) => None,
    };

    let mut opts = DiffOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);

    let mut diff = match spec {
        DiffSpec::WorkingTree => {
            repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut opts))?
        }
        DiffSpec::Staged => repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?,
    };

    let mut find_opts = DiffFindOptions::new();
    find_opts.renames(true);
    diff.find_similar(Some(&mut find_opts))?;

    let files_cell: RefCell<Vec<ChangedFile>> = RefCell::new(Vec::new());

    diff.foreach(
        &mut |delta, _progress| {
            let new_path = match delta.new_file().path() {
                Some(p) => p.to_path_buf(),
                None => return true,
            };
            let status = match delta.status() {
                Delta::Added => FileStatus::Added,
                Delta::Modified => FileStatus::Modified,
                Delta::Renamed => FileStatus::Renamed,
                Delta::Deleted => FileStatus::Deleted,
                Delta::Untracked => FileStatus::Untracked,
                _ => return true,
            };
            let old_content = match status {
                FileStatus::Added | FileStatus::Untracked => None,
                _ => {
                    let old_oid = delta.old_file().id();
                    if old_oid.is_zero() {
                        None
                    } else {
                        repo.find_blob(old_oid).ok().map(|b| b.content().to_vec())
                    }
                }
            };
            files_cell.borrow_mut().push(ChangedFile {
                path: new_path,
                status,
                new_content: None,
                old_content,
                changed_line_ranges: Vec::new(),
            });
            true
        },
        None,
        Some(&mut |delta, hunk| {
            let new_path = match delta.new_file().path() {
                Some(p) => p.to_path_buf(),
                None => return true,
            };
            let mut files = files_cell.borrow_mut();
            if let Some(file) = files.iter_mut().rev().find(|f| f.path == new_path) {
                let start = hunk.new_start();
                let lines = hunk.new_lines();
                if lines > 0 {
                    file.changed_line_ranges.push(start..start + lines);
                }
            }
            true
        }),
        None,
    )?;

    let mut files = files_cell.into_inner();

    for file in files.iter_mut() {
        if matches!(file.status, FileStatus::Deleted) {
            continue;
        }
        let abs = workdir.join(&file.path);
        if abs.is_file() {
            file.new_content = Some(std::fs::read(&abs)?);
        }
    }

    Ok(files)
}
