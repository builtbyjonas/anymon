//! Glob patterns with `.gitignore`-like semantics.
//!
//! - `*` and `?` never match `/`; `**` matches any number of directories.
//! - A pattern without a `/` matches at any depth: `*.rs`, `Makefile`.
//! - A pattern containing a `/` is relative to the project root: `src/**/*.rs`.
//!   A leading `/` anchors a pattern without other slashes: `/Cargo.toml`.
//! - A trailing `/` or a pattern naming an existing directory matches
//!   everything inside that directory: `src`, `assets/`.
//! - Absolute paths (`/home/me/shared/**`, `C:/shared/**`) are allowed as long
//!   as the literal part exists.

use std::fmt;
use std::path::{Path, PathBuf};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

/// What a pattern is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternKind {
    /// Selects files that trigger a task.
    Watch,
    /// Excludes files (a matching directory excludes its whole content).
    Ignore,
}

/// A compiled pattern: a literal base directory plus globs relative to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    /// The pattern as written by the user.
    pub source: String,
    /// Directory the globs are relative to.
    pub base: PathBuf,
    /// Globs matched against paths relative to `base`.
    pub globs: Vec<String>,
    /// Whether `base` itself matches.
    pub matches_base: bool,
    /// Whether matches can be nested more than one level below `base`.
    pub recursive: bool,
}

/// An invalid pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternError {
    pub pattern: String,
    pub message: String,
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid pattern `{}`: {}", self.pattern, self.message)
    }
}

impl std::error::Error for PatternError {}

fn has_glob_meta(text: &str) -> bool {
    text.contains(['*', '?', '[', ']', '{', '}'])
}

fn build_glob(glob: &str) -> Result<globset::Glob, globset::Error> {
    GlobBuilder::new(glob).literal_separator(true).build()
}

impl Pattern {
    /// Compile `pattern` relative to the project `root`.
    pub fn compile(pattern: &str, root: &Path, kind: PatternKind) -> Result<Pattern, PatternError> {
        let error = |message: &str| PatternError {
            pattern: pattern.to_string(),
            message: message.to_string(),
        };

        let mut text = pattern.trim().to_string();
        if cfg!(windows) {
            text = text.replace('\\', "/");
        }
        if text.is_empty() {
            return Err(error("pattern is empty"));
        }
        if text.starts_with('!') {
            return Err(error(
                "negated patterns are not supported; use `ignore` instead",
            ));
        }
        let dir_only = text.len() > 1 && text.ends_with('/');
        let trimmed = text.trim_end_matches('/');

        let (mut base, rest) = if trimmed.is_empty() {
            (root.to_path_buf(), String::new())
        } else if let Some((prefix, rest)) = split_absolute(trimmed, root) {
            (prefix, rest)
        } else if trimmed.contains('/') {
            (
                root.to_path_buf(),
                trimmed.trim_start_matches('/').to_string(),
            )
        } else if kind == PatternKind::Watch
            && !has_glob_meta(trimmed)
            && root.join(trimmed).is_dir()
        {
            (root.to_path_buf(), trimmed.to_string())
        } else {
            (root.to_path_buf(), format!("**/{trimmed}"))
        };

        // Move the literal leading components into the base directory.
        let components: Vec<&str> = rest
            .split('/')
            .filter(|c| !c.is_empty() && *c != ".")
            .collect();
        let literal = components.iter().take_while(|c| !has_glob_meta(c)).count();
        for component in &components[..literal] {
            if *component == ".." {
                base.pop();
            } else {
                base.push(component);
            }
        }
        let glob = components[literal..].join("/");

        let compiled = if glob.is_empty() {
            if dir_only || base.is_dir() || base.file_name().is_none() {
                Pattern {
                    source: pattern.to_string(),
                    base,
                    globs: vec!["**".to_string()],
                    matches_base: true,
                    recursive: true,
                }
            } else {
                let name = base
                    .file_name()
                    .expect("checked above")
                    .to_string_lossy()
                    .into_owned();
                base.pop();
                Pattern {
                    source: pattern.to_string(),
                    base,
                    globs: vec![escape_literal(&name)],
                    matches_base: false,
                    recursive: false,
                }
            }
        } else {
            let recursive = glob.contains('/') || glob.contains("**");
            let globs = if dir_only {
                vec![glob.clone(), format!("{glob}/**")]
            } else {
                vec![glob]
            };
            Pattern {
                source: pattern.to_string(),
                base,
                globs,
                matches_base: false,
                recursive: recursive || dir_only,
            }
        };

        for glob in &compiled.globs {
            build_glob(glob).map_err(|err| error(&err.kind().to_string()))?;
        }
        Ok(compiled)
    }

    /// The directory that has to be watched for this pattern.
    pub fn watch_base(&self) -> WatchBase {
        WatchBase {
            path: self.base.clone(),
            recursive: self.recursive,
        }
    }
}

/// Split an absolute pattern into its root prefix and the rest.
fn split_absolute(pattern: &str, root: &Path) -> Option<(PathBuf, String)> {
    if cfg!(windows) {
        let bytes = pattern.as_bytes();
        let drive = bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'/';
        return drive.then(|| {
            (
                PathBuf::from(format!("{}\\", &pattern[..2])),
                pattern[3..].to_string(),
            )
        });
    }
    if !pattern.starts_with('/') {
        return None;
    }
    // A leading slash anchors the pattern to the project root, like in
    // .gitignore. It only means an absolute path when it clearly names one.
    let literal: Vec<&str> = pattern
        .split('/')
        .take_while(|c| !has_glob_meta(c))
        .filter(|c| !c.is_empty())
        .collect();
    if literal.is_empty() {
        return None;
    }
    let relative = literal.join("/");
    if root.join(&relative).exists() || !Path::new("/").join(&relative).exists() {
        return None;
    }
    Some((
        PathBuf::from("/"),
        pattern.trim_start_matches('/').to_string(),
    ))
}

fn escape_literal(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if matches!(c, '*' | '?' | '[' | ']' | '{' | '}') {
            out.push('[');
            out.push(c);
            out.push(']');
        } else {
            out.push(c);
        }
    }
    out
}

/// A directory that needs to be watched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchBase {
    pub path: PathBuf,
    pub recursive: bool,
}

/// Reduce a list of watch bases to a minimal set.
///
/// Bases that do not exist yet are replaced by their closest existing
/// ancestor inside `root` (watched recursively, so the directory is noticed
/// when it is created). Missing bases outside `root` are returned separately.
/// Bases covered by a recursive ancestor are dropped.
pub fn merge_bases(bases: Vec<WatchBase>, root: &Path) -> (Vec<WatchBase>, Vec<PathBuf>) {
    let mut missing = Vec::new();
    let mut resolved = Vec::new();
    for mut base in bases {
        if !base.path.exists() {
            if !base.path.starts_with(root) {
                missing.push(base.path);
                continue;
            }
            while !base.path.exists() && base.path != root && base.path.pop() {}
            base.recursive = true;
        }
        resolved.push(base);
    }

    resolved.sort_by(|a, b| a.path.cmp(&b.path).then(b.recursive.cmp(&a.recursive)));
    let mut merged: Vec<WatchBase> = Vec::new();
    for base in resolved {
        let covered = merged
            .iter()
            .any(|m| m.path == base.path || (m.recursive && base.path.starts_with(&m.path)));
        if !covered {
            merged.push(base);
        }
    }
    missing.sort();
    missing.dedup();
    (merged, missing)
}

#[derive(Debug, Clone)]
struct Group {
    base: PathBuf,
    set: GlobSet,
    matches_base: bool,
}

/// A set of compiled patterns that can be matched against absolute paths.
#[derive(Debug, Clone, Default)]
pub struct PathSet {
    groups: Vec<Group>,
}

impl PathSet {
    /// Build a set that matches paths selected by any of `patterns`.
    pub fn new(patterns: &[Pattern]) -> Result<PathSet, PatternError> {
        Self::build(patterns, false)
    }

    /// Build a set for exclusion: a matching directory also excludes
    /// everything inside it.
    pub fn for_ignore(patterns: &[Pattern]) -> Result<PathSet, PatternError> {
        Self::build(patterns, true)
    }

    fn build(patterns: &[Pattern], contents: bool) -> Result<PathSet, PatternError> {
        let mut grouped: Vec<(PathBuf, GlobSetBuilder, bool)> = Vec::new();
        for pattern in patterns {
            let index = match grouped
                .iter()
                .position(|(base, _, _)| *base == pattern.base)
            {
                Some(index) => index,
                None => {
                    grouped.push((pattern.base.clone(), GlobSetBuilder::new(), false));
                    grouped.len() - 1
                }
            };
            let (_, builder, matches_base) = &mut grouped[index];
            *matches_base |= pattern.matches_base;
            for glob in &pattern.globs {
                let mut variants = vec![glob.clone()];
                if contents && glob != "**" && !glob.ends_with("/**") {
                    variants.push(format!("{glob}/**"));
                }
                for variant in variants {
                    let compiled = build_glob(&variant).map_err(|err| PatternError {
                        pattern: pattern.source.clone(),
                        message: err.kind().to_string(),
                    })?;
                    builder.add(compiled);
                }
            }
        }

        let mut groups = Vec::with_capacity(grouped.len());
        for (base, builder, matches_base) in grouped {
            let set = builder.build().map_err(|err| PatternError {
                pattern: base.display().to_string(),
                message: err.to_string(),
            })?;
            groups.push(Group {
                base,
                set,
                matches_base,
            });
        }
        Ok(PathSet { groups })
    }

    /// Returns `true` if the set contains no patterns.
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// Returns `true` if `path` (absolute) matches any pattern.
    pub fn is_match(&self, path: &Path) -> bool {
        self.groups
            .iter()
            .any(|group| match path.strip_prefix(&group.base) {
                Ok(rel) if rel.as_os_str().is_empty() => group.matches_base,
                Ok(rel) => group.set.is_match(rel),
                Err(_) => false,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let root = crate::paths::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("src/nested")).unwrap();
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "").unwrap();
        Fixture { _dir: dir, root }
    }

    fn watch_set(root: &Path, patterns: &[&str]) -> PathSet {
        let compiled: Vec<_> = patterns
            .iter()
            .map(|p| Pattern::compile(p, root, PatternKind::Watch).unwrap())
            .collect();
        PathSet::new(&compiled).unwrap()
    }

    fn ignore_set(root: &Path, patterns: &[&str]) -> PathSet {
        let compiled: Vec<_> = patterns
            .iter()
            .map(|p| Pattern::compile(p, root, PatternKind::Ignore).unwrap())
            .collect();
        PathSet::for_ignore(&compiled).unwrap()
    }

    #[test]
    fn patterns_without_slash_match_at_any_depth() {
        let f = fixture();
        let set = watch_set(&f.root, &["*.rs"]);
        assert!(set.is_match(&f.root.join("main.rs")));
        assert!(set.is_match(&f.root.join("src/nested/deep.rs")));
        assert!(!set.is_match(&f.root.join("src/main.rs.bak")));

        let set = watch_set(&f.root, &["Makefile"]);
        assert!(set.is_match(&f.root.join("Makefile")));
        assert!(set.is_match(&f.root.join("sub/Makefile")));
        assert!(!set.is_match(&f.root.join("Makefile.am")));
    }

    #[test]
    fn patterns_with_slash_are_anchored() {
        let f = fixture();
        let set = watch_set(&f.root, &["src/*.rs"]);
        assert!(set.is_match(&f.root.join("src/main.rs")));
        assert!(!set.is_match(&f.root.join("src/nested/deep.rs")));
        assert!(!set.is_match(&f.root.join("other/src/main.rs")));

        let set = watch_set(&f.root, &["src/**/*.rs"]);
        assert!(set.is_match(&f.root.join("src/main.rs")));
        assert!(set.is_match(&f.root.join("src/nested/deep.rs")));

        let set = watch_set(&f.root, &["/Cargo.toml"]);
        assert!(set.is_match(&f.root.join("Cargo.toml")));
        assert!(!set.is_match(&f.root.join("crates/x/Cargo.toml")));

        let set = watch_set(&f.root, &["./src/**"]);
        assert!(set.is_match(&f.root.join("src/nested/deep.rs")));
    }

    #[test]
    fn directories_match_their_contents() {
        let f = fixture();
        for pattern in ["src", "src/", "/src", "src/**"] {
            let set = watch_set(&f.root, &[pattern]);
            assert!(set.is_match(&f.root.join("src/main.rs")), "{pattern}");
            assert!(
                set.is_match(&f.root.join("src/nested/deep.rs")),
                "{pattern}"
            );
            assert!(!set.is_match(&f.root.join("main.rs")), "{pattern}");
        }
        // A literal name that is not a directory at the root is a file name.
        let set = watch_set(&f.root, &["build/"]);
        assert!(set.is_match(&f.root.join("build/out.js")));
        assert!(set.is_match(&f.root.join("x/build/out.js")));
    }

    #[test]
    fn narrows_watch_bases() {
        let f = fixture();
        let base = |p: &str| {
            Pattern::compile(p, &f.root, PatternKind::Watch)
                .unwrap()
                .watch_base()
        };
        assert_eq!(
            base("src/**"),
            WatchBase {
                path: f.root.join("src"),
                recursive: true
            }
        );
        assert_eq!(
            base("src/*.rs"),
            WatchBase {
                path: f.root.join("src"),
                recursive: false
            }
        );
        assert_eq!(
            base("src"),
            WatchBase {
                path: f.root.join("src"),
                recursive: true
            }
        );
        assert_eq!(
            base("/Cargo.toml"),
            WatchBase {
                path: f.root.clone(),
                recursive: false
            }
        );
        assert_eq!(
            base("src/nested/x.rs"),
            WatchBase {
                path: f.root.join("src/nested"),
                recursive: false
            }
        );
        assert_eq!(
            base("*.rs"),
            WatchBase {
                path: f.root.clone(),
                recursive: true
            }
        );
        assert_eq!(
            base("**"),
            WatchBase {
                path: f.root.clone(),
                recursive: true
            }
        );
    }

    #[test]
    fn parent_directories_and_absolute_paths() {
        let f = fixture();
        let project = f.root.join("src");
        let pattern = Pattern::compile("../assets/**", &project, PatternKind::Watch).unwrap();
        assert_eq!(pattern.base, f.root.join("assets"));

        let absolute = format!(
            "{}/**/*.rs",
            f.root.join("src").to_string_lossy().replace('\\', "/")
        );
        let other_root = f.root.join("assets");
        let set = watch_set(&other_root, &[absolute.as_str()]);
        assert!(set.is_match(&f.root.join("src/nested/deep.rs")));
        assert!(!set.is_match(&other_root.join("x.rs")));
    }

    #[test]
    fn ignore_sets_exclude_directory_contents() {
        let f = fixture();
        let set = ignore_set(&f.root, &["target", "*.log", "dist/"]);
        assert!(set.is_match(&f.root.join("target")));
        assert!(set.is_match(&f.root.join("target/debug/app")));
        assert!(set.is_match(&f.root.join("crates/a/target/x")));
        assert!(set.is_match(&f.root.join("logs/today.log")));
        assert!(set.is_match(&f.root.join("dist/app.js")));
        assert!(!set.is_match(&f.root.join("src/main.rs")));
        assert!(!set.is_match(&f.root.join("targets.txt")));

        let set = ignore_set(&f.root, &["src"]);
        assert!(set.is_match(&f.root.join("src")));
        assert!(set.is_match(&f.root.join("src/nested/deep.rs")));
    }

    #[test]
    fn braces_and_classes() {
        let f = fixture();
        let set = watch_set(&f.root, &["*.{js,ts}", "img/[a-c]*.png"]);
        assert!(set.is_match(&f.root.join("app.ts")));
        assert!(set.is_match(&f.root.join("lib/app.js")));
        assert!(!set.is_match(&f.root.join("app.rs")));
        assert!(set.is_match(&f.root.join("img/b1.png")));
        assert!(!set.is_match(&f.root.join("img/d1.png")));
    }

    #[test]
    fn rejects_invalid_patterns() {
        let f = fixture();
        let err = Pattern::compile("src/[", &f.root, PatternKind::Watch).unwrap_err();
        assert!(
            err.to_string().starts_with("invalid pattern `src/[`"),
            "{err}"
        );
        let err = Pattern::compile("!src/**", &f.root, PatternKind::Watch).unwrap_err();
        assert!(err.to_string().contains("use `ignore` instead"), "{err}");
        assert!(Pattern::compile("  ", &f.root, PatternKind::Watch).is_err());
    }

    #[test]
    fn merges_watch_bases() {
        let f = fixture();
        let b = |p: &Path, recursive| WatchBase {
            path: p.to_path_buf(),
            recursive,
        };
        let (merged, missing) = merge_bases(
            vec![
                b(&f.root.join("src/nested"), true),
                b(&f.root.join("src"), false),
                b(&f.root.join("src"), true),
                b(&f.root, false),
                b(&f.root.join("assets"), false),
                b(&f.root.join("assets"), false),
                b(&f.root.join("not-yet/created"), false),
                b(&f.root.parent().unwrap().join("anymon-missing-dir"), true),
            ],
            &f.root,
        );
        assert_eq!(
            merged,
            vec![b(&f.root, true),],
            "the missing directory inside the root forces a recursive root watch"
        );
        assert_eq!(
            missing,
            vec![f.root.parent().unwrap().join("anymon-missing-dir")]
        );

        let (merged, _) = merge_bases(
            vec![
                b(&f.root.join("src/nested"), true),
                b(&f.root.join("src"), false),
                b(&f.root.join("src"), true),
                b(&f.root, false),
            ],
            &f.root,
        );
        assert_eq!(
            merged,
            vec![b(&f.root, false), b(&f.root.join("src"), true)]
        );
    }
}
