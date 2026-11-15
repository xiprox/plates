//! The branch a session's folder has checked out, which the hover card shows beside the folder.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// The branch checked out in the repository that holds `folder`, from its `HEAD` file, or the
/// commit's first 7 characters when no branch is checked out. `None` means the folder isn't in a
/// repository, or its `HEAD` can't be read.
pub fn branch(folder: &Path) -> Option<String> {
    let head = fs::read_to_string(git_dir(folder)?.join("HEAD")).ok()?;
    let head = head.trim();
    match head.strip_prefix("ref:") {
        Some(name) => {
            let name = name.trim();
            Some(name.strip_prefix("refs/heads/").unwrap_or(name).to_string())
        }
        None => head.get(..7).map(str::to_string),
    }
}

// A worktree's `.git` is a file, `gitdir: <path>`, naming the folder that holds the worktree's own
// `HEAD`, and a relative path there is relative to the worktree.
fn git_dir(folder: &Path) -> Option<PathBuf> {
    let dot_git = folder
        .ancestors()
        .map(|d| d.join(".git"))
        .find(|p| p.exists())?;
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    let text = fs::read_to_string(&dot_git).ok()?;
    let path = Path::new(text.trim().strip_prefix("gitdir:")?.trim());
    Some(dot_git.parent()?.join(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("plates-core-git-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_folder_inside_a_repository_names_its_branch() {
        let repo = folder("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::write(repo.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::create_dir_all(repo.join("src")).unwrap();
        assert_eq!(branch(&repo.join("src")).as_deref(), Some("main"));

        fs::write(repo.join(".git").join("HEAD"), "c20bf1c0d6e1a2b3\n").unwrap();
        assert_eq!(branch(&repo).as_deref(), Some("c20bf1c"), "a detached HEAD");
    }

    #[test]
    fn a_worktree_reads_the_head_its_git_file_names() {
        let root = folder("worktree");
        let own = root
            .join("main")
            .join(".git")
            .join("worktrees")
            .join("card");
        fs::create_dir_all(&own).unwrap();
        fs::write(own.join("HEAD"), "ref: refs/heads/claude/card\n").unwrap();
        let tree = root.join("card");
        fs::create_dir_all(&tree).unwrap();
        fs::write(tree.join(".git"), "gitdir: ../main/.git/worktrees/card\n").unwrap();
        assert_eq!(branch(&tree).as_deref(), Some("claude/card"));
    }
}
