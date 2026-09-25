//! Path helpers.

use std::io;
use std::path::{Path, PathBuf};

/// Canonicalize a path without the `\\?\` prefix Windows adds, which many
/// programs (including `cmd.exe` as a working directory) do not support.
pub fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    std::fs::canonicalize(path).map(simplify)
}

#[cfg(windows)]
fn simplify(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        let bytes = rest.as_bytes();
        if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            return PathBuf::from(rest);
        }
    }
    path
}

#[cfg(not(windows))]
fn simplify(path: PathBuf) -> PathBuf {
    path
}

/// Canonicalize `path` if it exists, otherwise make it absolute relative to
/// `base` without touching the file system.
pub fn resolve(path: &Path, base: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    canonicalize(&joined).unwrap_or(joined)
}

/// Display `path` relative to `root` when it is inside it.
pub fn display_relative(path: &Path, root: &Path) -> String {
    match path.strip_prefix(root) {
        Ok(rel) if rel.as_os_str().is_empty() => ".".to_string(),
        Ok(rel) => rel.display().to_string(),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_existing_paths() {
        let dir = tempfile::tempdir().unwrap();
        let canonical = canonicalize(dir.path()).unwrap();
        assert!(canonical.is_absolute());
        assert!(!canonical.to_string_lossy().starts_with(r"\\?\"));
        assert_eq!(canonicalize(&canonical.join(".")).unwrap(), canonical);
    }

    #[test]
    fn resolves_missing_paths_lexically() {
        let dir = tempfile::tempdir().unwrap();
        let base = canonicalize(dir.path()).unwrap();
        assert_eq!(
            resolve(Path::new("missing/x"), &base),
            base.join("missing/x")
        );
        assert_eq!(resolve(Path::new("."), &base), base);
    }

    #[test]
    fn displays_relative_paths() {
        let root = Path::new("/project");
        assert_eq!(display_relative(Path::new("/project"), root), ".");
        assert_eq!(
            display_relative(Path::new("/project/src/main.rs"), root),
            Path::new("src").join("main.rs").display().to_string()
        );
        assert_eq!(
            display_relative(Path::new("/elsewhere/x"), root),
            "/elsewhere/x"
        );
    }
}
