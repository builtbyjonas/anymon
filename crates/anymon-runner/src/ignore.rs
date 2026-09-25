//! Built-in ignore rules and `.gitignore` support.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::Match;

/// Version-control directories that are never interesting.
const VCS_DIRS: &[&str] = &[".git", ".hg", ".svn", ".jj", ".bzr", "_darcs"];

/// Returns `true` for paths that no watcher should react to: VCS metadata,
/// OS junk and editor swap/backup files. `root` limits which parent
/// directories are considered.
pub fn is_builtin_ignored(path: &Path, root: &Path) -> bool {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let in_vcs_dir = rel.components().any(|c| match c {
        Component::Normal(name) => VCS_DIRS.iter().any(|d| OsStr::new(d) == name),
        _ => false,
    });
    if in_vcs_dir {
        return true;
    }
    let Some(name) = path.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    matches!(name, ".DS_Store" | "Thumbs.db" | "desktop.ini" | "4913")
        || name.ends_with(".swp")
        || name.ends_with(".swo")
        || name.ends_with(".swx")
        || name.ends_with('~')
        || name.starts_with(".#")
        || (name.len() > 1 && name.starts_with('#') && name.ends_with('#'))
        || name.ends_with("___jb_tmp___")
        || name.ends_with("___jb_old___")
}

/// All `.gitignore` files that apply to the watched directories.
#[derive(Debug, Default)]
pub struct GitignoreTree {
    matchers: HashMap<PathBuf, Gitignore>,
}

impl GitignoreTree {
    /// Create an empty tree.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load (or reload) the ignore files of `dir`: `.gitignore`, plus
    /// `.git/info/exclude` when `dir` is a repository root.
    pub fn load_dir(&mut self, dir: &Path) {
        let mut builder = GitignoreBuilder::new(dir);
        let mut found = false;
        for file in [
            dir.join(".gitignore"),
            dir.join(".git").join("info").join("exclude"),
        ] {
            if file.is_file() {
                found = true;
                // Invalid lines are skipped; the valid ones still apply.
                let _ = builder.add(file);
            }
        }
        match builder.build() {
            Ok(matcher) if found && !matcher.is_empty() => {
                self.matchers.insert(dir.to_path_buf(), matcher);
            }
            _ => {
                self.matchers.remove(dir);
            }
        }
    }

    /// Load the ignore files of the directories above `root`, up to the
    /// enclosing repository root. Nothing is loaded outside a repository.
    pub fn load_ancestors(&mut self, root: &Path) {
        let Some(repo) = root.ancestors().find(|dir| dir.join(".git").exists()) else {
            return;
        };
        for dir in root.ancestors().skip(1) {
            if !dir.starts_with(repo) {
                break;
            }
            self.load_dir(dir);
        }
    }

    /// Load the ignore files of every directory from `root` down to `path`.
    pub fn load_between(&mut self, root: &Path, path: &Path) {
        if !path.starts_with(root) {
            return;
        }
        for dir in path.ancestors() {
            if dir.is_dir() && !self.matchers.contains_key(dir) {
                self.load_dir(dir);
            }
            if dir == root {
                break;
            }
        }
    }

    /// Returns `true` if a loaded `.gitignore` excludes `path` or one of its
    /// parent directories. Deeper files take precedence.
    pub fn is_ignored(&self, path: &Path, is_dir: bool) -> bool {
        if self.matchers.is_empty() {
            return false;
        }
        for dir in path.ancestors().skip(1) {
            if let Some(matcher) = self.matchers.get(dir) {
                match matcher.matched_path_or_any_parents(path, is_dir) {
                    Match::Ignore(_) => return true,
                    Match::Whitelist(_) => return false,
                    Match::None => {}
                }
            }
        }
        false
    }

    /// Number of loaded ignore files.
    pub fn len(&self) -> usize {
        self.matchers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_rules() {
        let root = Path::new("/p");
        for ignored in [
            "/p/.git/index",
            "/p/.git",
            "/p/sub/.hg/store",
            "/p/.DS_Store",
            "/p/src/.main.rs.swp",
            "/p/src/main.rs~",
            "/p/src/.#main.rs",
            "/p/src/#main.rs#",
            "/p/src/4913",
            "/p/src/main.rs___jb_tmp___",
        ] {
            assert!(is_builtin_ignored(Path::new(ignored), root), "{ignored}");
        }
        for kept in [
            "/p/src/main.rs",
            "/p/.github/workflows/ci.yml",
            "/p/.gitignore",
            "/p/#",
        ] {
            assert!(!is_builtin_ignored(Path::new(kept), root), "{kept}");
        }
        // Parents above the root do not count.
        assert!(!is_builtin_ignored(
            Path::new("/x/.git/p/src/a.rs"),
            Path::new("/x/.git/p")
        ));
    }

    fn tree_with(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf, GitignoreTree) {
        let dir = tempfile::tempdir().unwrap();
        let root = crate::paths::canonicalize(dir.path()).unwrap();
        for (path, content) in files {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        let mut tree = GitignoreTree::new();
        tree.load_dir(&root);
        tree.load_dir(&root.join("web"));
        (dir, root, tree)
    }

    #[test]
    fn gitignore_rules_apply_to_contents() {
        let (_dir, root, tree) = tree_with(&[
            (".gitignore", "target/\n*.log\n/dist\n!keep.log\n"),
            ("web/.gitignore", "node_modules\n"),
        ]);
        assert_eq!(tree.len(), 2);
        assert!(tree.is_ignored(&root.join("target"), true));
        assert!(tree.is_ignored(&root.join("target/debug/app"), false));
        assert!(tree.is_ignored(&root.join("crates/x/target/a.rs"), false));
        assert!(tree.is_ignored(&root.join("build.log"), false));
        assert!(!tree.is_ignored(&root.join("keep.log"), false));
        assert!(tree.is_ignored(&root.join("dist/app.js"), false));
        assert!(!tree.is_ignored(&root.join("web/dist/app.js"), false));
        assert!(tree.is_ignored(&root.join("web/node_modules/x/index.js"), false));
        assert!(!tree.is_ignored(&root.join("node_modules/x/index.js"), false));
        assert!(!tree.is_ignored(&root.join("src/main.rs"), false));
    }

    #[test]
    fn deeper_files_override_shallower_ones() {
        let (_dir, root, tree) = tree_with(&[
            (".gitignore", "*.gen.ts\n"),
            ("web/.gitignore", "!api.gen.ts\n"),
        ]);
        assert!(tree.is_ignored(&root.join("a.gen.ts"), false));
        assert!(tree.is_ignored(&root.join("web/b.gen.ts"), false));
        assert!(!tree.is_ignored(&root.join("web/api.gen.ts"), false));
    }

    #[test]
    fn reloading_picks_up_changes() {
        let (_dir, root, mut tree) = tree_with(&[(".gitignore", "*.tmp\n")]);
        assert!(tree.is_ignored(&root.join("a.tmp"), false));
        std::fs::write(root.join(".gitignore"), "*.bak\n").unwrap();
        tree.load_dir(&root);
        assert!(!tree.is_ignored(&root.join("a.tmp"), false));
        assert!(tree.is_ignored(&root.join("a.bak"), false));
        std::fs::remove_file(root.join(".gitignore")).unwrap();
        tree.load_dir(&root);
        assert_eq!(tree.len(), 0);
    }

    #[test]
    fn loads_ancestors_up_to_the_repository_root() {
        let (_dir, root, _) = tree_with(&[
            (".gitignore", "*.log\n"),
            ("app/.gitignore", "*.tmp\n"),
            ("app/service/main.rs", ""),
        ]);
        std::fs::create_dir(root.join(".git")).unwrap();
        let service = root.join("app/service");

        let mut tree = GitignoreTree::new();
        tree.load_ancestors(&service);
        assert_eq!(tree.len(), 2);
        assert!(tree.is_ignored(&service.join("x.log"), false));
        assert!(tree.is_ignored(&service.join("x.tmp"), false));

        let mut tree = GitignoreTree::new();
        tree.load_between(&root, &service);
        assert_eq!(tree.len(), 2);
    }

    #[test]
    fn nothing_is_loaded_outside_a_repository() {
        let (_dir, root, _) = tree_with(&[(".gitignore", "*.log\n"), ("app/main.rs", "")]);
        let mut tree = GitignoreTree::new();
        tree.load_ancestors(&root.join("app"));
        assert_eq!(tree.len(), 0);
    }
}
