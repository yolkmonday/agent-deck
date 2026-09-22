use std::path::{Path, PathBuf};

use crate::live::project_name;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    pub root: PathBuf,
    pub is_worktree: bool,
    pub worktree_name: Option<String>,
}

fn own(cwd: &Path) -> RepoInfo {
    RepoInfo { root: cwd.to_path_buf(), is_worktree: false, worktree_name: None }
}

/// Reads `<dir>/.git` and returns the repo it points at, if that directory is a
/// checkout at all. A linked worktree's `.git` is a file, not a directory.
fn inspect(dir: &Path) -> Option<RepoInfo> {
    let git = dir.join(".git");
    if git.is_dir() {
        return Some(own(dir));
    }
    let raw = std::fs::read_to_string(&git).ok()?;
    let target = raw
        .lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix("gitdir:"))?
        .trim();
    let path = Path::new(target);
    let worktrees = worktrees_root(path)?;
    let root = worktrees.parent()?.parent()?;
    let name = path.strip_prefix(&worktrees).ok()?.to_str()?.trim_matches('/').to_string();
    Some(RepoInfo {
        root: root.to_path_buf(),
        is_worktree: true,
        worktree_name: (!name.is_empty()).then_some(name),
    })
}

/// The prefix of `path` up to and including the first `.git/worktrees` pair,
/// which is the directory a linked worktree's name hangs off.
fn worktrees_root(path: &Path) -> Option<PathBuf> {
    let comps: Vec<_> = path.components().collect();
    let at = comps
        .windows(2)
        .position(|w| w[0].as_os_str() == ".git" && w[1].as_os_str() == "worktrees")?;
    Some(comps[..=at + 1].iter().collect())
}

pub fn resolve(cwd: &Path) -> RepoInfo {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        if let Some(info) = inspect(d) {
            return info;
        }
        dir = d.parent();
    }
    own(cwd)
}

pub fn display_name(root: &Path, home: &str) -> String {
    project_name(&root.to_string_lossy(), home)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn dir_with_git_dir(parent: &Path, name: &str) -> PathBuf {
        let d = parent.join(name);
        fs::create_dir_all(d.join(".git")).unwrap();
        d
    }

    fn worktree_fixture(parent: &Path, main: &str, name: &str) -> PathBuf {
        let main_repo = parent.join(main);
        let gitdir = main_repo.join(".git/worktrees").join(name);
        fs::create_dir_all(&gitdir).unwrap();
        let wt = parent.join(name);
        fs::create_dir_all(&wt).unwrap();
        fs::write(wt.join(".git"), format!("gitdir: {}\n", gitdir.display())).unwrap();
        wt
    }

    #[test]
    fn plain_repo_is_its_own_root() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = dir_with_git_dir(tmp.path(), "main");
        let info = resolve(&repo);
        assert_eq!(info.root, repo);
        assert!(!info.is_worktree);
        assert_eq!(info.worktree_name, None);
    }

    #[test]
    fn worktree_resolves_to_the_main_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let main = tmp.path().join("main");
        fs::create_dir_all(main.join(".git/worktrees/feat-1")).unwrap();
        let wt = tmp.path().join("wt");
        fs::create_dir_all(&wt).unwrap();
        fs::write(wt.join(".git"), "gitdir: /tmp/x/main/.git/worktrees/feat-1").unwrap();

        let info = resolve(&wt);
        assert_eq!(info.root, PathBuf::from("/tmp/x/main"));
        assert!(info.is_worktree);
        assert_eq!(info.worktree_name.as_deref(), Some("feat-1"));
    }

    #[test]
    fn worktree_file_with_trailing_newline_and_spaces_parses() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = tmp.path().join("wt");
        fs::create_dir_all(&wt).unwrap();
        fs::write(wt.join(".git"), "  gitdir:  /tmp/x/main/.git/worktrees/feat-1  \n\n").unwrap();

        let info = resolve(&wt);
        assert_eq!(info.root, PathBuf::from("/tmp/x/main"));
        assert!(info.is_worktree);
        assert_eq!(info.worktree_name.as_deref(), Some("feat-1"));
    }

    #[test]
    fn gitdir_without_the_worktrees_segment_is_not_a_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path().join("sub");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join(".git"), "gitdir: /tmp/x/main/.git\n").unwrap();

        let info = resolve(&d);
        assert_eq!(info.root, d);
        assert!(!info.is_worktree);
        assert_eq!(info.worktree_name, None);
    }

    #[test]
    fn subdirectory_of_a_repo_walks_up() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = dir_with_git_dir(tmp.path(), "main");
        let deep = repo.join("src/deep");
        fs::create_dir_all(&deep).unwrap();

        let info = resolve(&deep);
        assert_eq!(info.root, repo);
        assert!(!info.is_worktree);
    }

    #[test]
    fn subdirectory_of_a_worktree_walks_up_to_the_main_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let wt = worktree_fixture(tmp.path(), "main", "feat-1");
        let deep = wt.join("src/deep");
        fs::create_dir_all(&deep).unwrap();

        let info = resolve(&deep);
        assert_eq!(info.root, tmp.path().join("main"));
        assert!(info.is_worktree);
        assert_eq!(info.worktree_name.as_deref(), Some("feat-1"));
    }

    #[test]
    fn a_non_repo_directory_is_its_own_root() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path().join("plain/deep");
        fs::create_dir_all(&d).unwrap();

        let info = resolve(&d);
        assert_eq!(info.root, d);
        assert!(!info.is_worktree);
        assert_eq!(info.worktree_name, None);
    }

    #[test]
    fn resolution_stops_at_the_filesystem_root_and_does_not_loop() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path().join("a/b/c");
        fs::create_dir_all(&d).unwrap();

        let info = resolve(&d);
        assert_eq!(info.root, d);
        assert_eq!(display_name(&info.root, "/nope/home"), "c");
        assert_eq!(display_name(Path::new("/Users/yolk"), "/Users/yolk"), "~");
    }
}
