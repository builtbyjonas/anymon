//! `anymon update`: replace the running binary with the latest release.

use std::cmp::Ordering;
use std::fmt;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use anymon_runner::ui;
use sha2::{Digest, Sha256};

const REPO: &str = "builtbyjonas/anymon";
/// The Rust target triple this binary was built for (set by build.rs).
pub const TARGET: &str = env!("ANYMON_TARGET");
const MAX_DOWNLOAD: u64 = 256 * 1024 * 1024;

/// A `major.minor.patch[-pre]` version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Option<String>,
}

impl Version {
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.trim().trim_start_matches('v');
        let text = text.split('+').next()?;
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre.to_string())),
            None => (text, None),
        };
        let mut numbers = core.split('.').map(|n| n.parse::<u64>().ok());
        let version = Version {
            major: numbers.next()??,
            minor: numbers.next()??,
            patch: numbers.next()??,
            pre,
        };
        numbers.next().is_none().then_some(version)
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => a.cmp(b),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.pre {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

/// File name of the release archive for `target`.
pub fn archive_name(target: &str) -> String {
    let ext = if target.contains("windows") {
        "zip"
    } else {
        "tar.gz"
    };
    format!("anymon-{target}.{ext}")
}

fn binary_name(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// How anymon was installed, if a package manager owns the binary.
fn package_manager(exe: &Path) -> Option<&'static str> {
    exe.components()
        .any(|c| c.as_os_str() == "node_modules")
        .then_some("npm")
}

pub fn run(check_only: bool) -> Result<i32> {
    let current = Version::parse(env!("CARGO_PKG_VERSION")).expect("valid package version");
    let exe = std::env::current_exe().context("cannot locate the anymon executable")?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);

    let agent = agent();
    ui::info("checking for updates");
    let tag = latest_tag(&agent)?;
    let latest = Version::parse(&tag).ok_or_else(|| anyhow!("unexpected release tag `{tag}`"))?;

    if latest <= current {
        ui::info(format!("anymon v{current} is up to date"));
        return Ok(0);
    }

    let manager = package_manager(&exe);
    if check_only || manager.is_some() {
        ui::info(format!(
            "anymon v{latest} is available (installed: v{current})"
        ));
        match manager {
            Some(_) => {
                ui::info("anymon was installed with npm; update it with `npm i -g anymon@latest`")
            }
            None => ui::info("run `anymon update` to install it"),
        }
        return Ok(0);
    }

    let archive = archive_name(TARGET);
    let url = format!("https://github.com/{REPO}/releases/download/{tag}/{archive}");
    ui::info(format!("downloading {archive} ({tag})"));
    let data = download(&agent, &url).map_err(|err| match err.downcast_ref::<ureq::Error>() {
        Some(ureq::Error::StatusCode(404)) => {
            anyhow!("release {tag} has no prebuilt binary for {TARGET}; see https://github.com/{REPO}/blob/main/docs/installation.md")
        }
        _ => err,
    })?;
    verify_checksum(&agent, &url, &data)?;

    let binaries = extract(&archive, &data, &["anymon", "anymon-shell"])?;
    let dir = exe
        .parent()
        .context("the executable has no parent directory")?;
    for (name, content) in &binaries {
        let target = if name == "anymon" {
            exe.clone()
        } else {
            dir.join(binary_name(name))
        };
        // Only replace companion binaries that are already installed.
        if name != "anymon" && !target.exists() {
            continue;
        }
        replace_file(&target, content)?;
    }
    if !binaries.iter().any(|(name, _)| name == "anymon") {
        bail!("{archive} does not contain the anymon binary");
    }

    ui::info(format!(
        "updated anymon v{current} -> v{latest} ({})",
        exe.display()
    ));
    Ok(0)
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .user_agent(concat!("anymon/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Find the tag of the latest release. The redirect of `/releases/latest`
/// is used first because it is not rate limited; the API is the fallback.
fn latest_tag(agent: &ureq::Agent) -> Result<String> {
    let url = format!("https://github.com/{REPO}/releases/latest");
    let redirect = agent
        .get(&url)
        .config()
        .max_redirects(0)
        .build()
        .call()
        .ok()
        .and_then(|response| {
            let location = response.headers().get("location")?.to_str().ok()?;
            location
                .rsplit_once("/tag/")
                .map(|(_, tag)| tag.to_string())
        });
    if let Some(tag) = redirect.filter(|t| !t.is_empty()) {
        return Ok(tag);
    }

    let api = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let mut request = agent
        .get(&api)
        .header("Accept", "application/vnd.github+json");
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.is_empty() {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }
    }
    let body = request
        .call()
        .context("cannot reach GitHub to check for updates")?
        .into_body()
        .with_config()
        .limit(MAX_DOWNLOAD)
        .read_to_string()
        .context("cannot read the release information")?;
    let json: serde_json::Value =
        serde_json::from_str(&body).context("unexpected release information")?;
    json["tag_name"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow!("no published release found"))
}

fn download(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>> {
    let response = agent.get(url).call()?;
    Ok(response
        .into_body()
        .with_config()
        .limit(MAX_DOWNLOAD)
        .read_to_vec()?)
}

fn verify_checksum(agent: &ureq::Agent, url: &str, data: &[u8]) -> Result<()> {
    let expected = match agent.get(&format!("{url}.sha256")).call() {
        Ok(response) => response.into_body().read_to_string()?,
        Err(ureq::Error::StatusCode(404)) => {
            ui::warn("this release publishes no checksum; skipping verification");
            return Ok(());
        }
        Err(err) => return Err(err).context("cannot download the checksum"),
    };
    let expected = expected
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("the published checksum is empty"))?
        .to_ascii_lowercase();
    let actual = sha256_hex(data);
    if actual != expected {
        bail!("checksum mismatch for the download (expected {expected}, got {actual}); nothing was changed");
    }
    ui::detail("checksum verified");
    Ok(())
}

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Extract the named binaries (without extension) from a release archive.
pub fn extract(archive: &str, data: &[u8], names: &[&str]) -> Result<Vec<(String, Vec<u8>)>> {
    let wanted = |path: &Path| -> Option<String> {
        let file = path.file_name()?.to_str()?;
        names
            .iter()
            .find(|name| file == binary_name(name) || file == **name)
            .map(|name| name.to_string())
    };

    let mut found = Vec::new();
    if archive.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(Cursor::new(data)).context("invalid zip archive")?;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index)?;
            if !entry.is_file() {
                continue;
            }
            let Some(name) = entry.enclosed_name().as_deref().and_then(wanted) else {
                continue;
            };
            let mut content = Vec::new();
            entry.read_to_end(&mut content)?;
            found.push((name, content));
        }
    } else {
        let decoder = flate2::read::GzDecoder::new(data);
        let mut tar = tar::Archive::new(decoder);
        for entry in tar.entries().context("invalid tar.gz archive")? {
            let mut entry = entry?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let Some(name) = wanted(&entry.path()?) else {
                continue;
            };
            let mut content = Vec::new();
            entry.read_to_end(&mut content)?;
            found.push((name, content));
        }
    }
    Ok(found)
}

/// Atomically replace `target` with `content`.
///
/// The new file is written next to the target and renamed over it, so a
/// failed update never leaves a broken binary behind. Windows does not allow
/// replacing a running executable, but it allows renaming it, so the old
/// binary is moved aside to `<name>.old` and removed on the next start.
pub fn replace_file(target: &Path, content: &[u8]) -> Result<()> {
    let dir = target.parent().context("target has no parent directory")?;
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .context("invalid target file name")?;
    let staged = dir.join(format!(".{name}.new"));
    std::fs::write(&staged, content).map_err(|err| permission_hint(err, dir))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
    }

    #[cfg(windows)]
    {
        let old = dir.join(format!("{name}.old"));
        let _ = std::fs::remove_file(&old);
        if target.exists() {
            if let Err(err) = std::fs::rename(target, &old) {
                let _ = std::fs::remove_file(&staged);
                return Err(permission_hint(err, dir));
            }
        }
        if let Err(err) = std::fs::rename(&staged, target) {
            let _ = std::fs::rename(&old, target);
            return Err(permission_hint(err, dir));
        }
        Ok(())
    }

    #[cfg(not(windows))]
    {
        std::fs::rename(&staged, target).map_err(|err| {
            let _ = std::fs::remove_file(&staged);
            permission_hint(err, dir)
        })
    }
}

fn permission_hint(err: std::io::Error, dir: &Path) -> anyhow::Error {
    if err.kind() == std::io::ErrorKind::PermissionDenied {
        anyhow!(
            "no permission to write to {}; re-run with elevated rights or reinstall anymon",
            dir.display()
        )
    } else {
        anyhow::Error::new(err).context(format!("cannot write to {}", dir.display()))
    }
}

/// Remove the binary a previous update moved aside (Windows only).
pub fn cleanup_previous_update() {
    if !cfg!(windows) {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut old = exe.into_os_string();
        old.push(".old");
        let old = PathBuf::from(old);
        if old.exists() {
            let _ = std::fs::remove_file(old);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_orders_versions() {
        let v = |s| Version::parse(s).unwrap();
        assert_eq!(v("1.2.3").to_string(), "1.2.3");
        assert_eq!(v("v1.0.0-rc.1+build.5").to_string(), "1.0.0-rc.1");
        assert!(v("1.0.0") > v("0.7.2"));
        assert!(v("0.10.0") > v("0.9.9"));
        assert!(v("1.0.0") > v("1.0.0-rc.2"));
        assert!(v("1.0.0-rc.2") > v("1.0.0-rc.1"));
        assert_eq!(v("v2.0.0"), v("2.0.0"));
        for invalid in ["", "1", "1.2", "1.2.x", "1.2.3.4", "latest"] {
            assert_eq!(Version::parse(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn names_archives_by_target() {
        assert_eq!(
            archive_name("x86_64-unknown-linux-musl"),
            "anymon-x86_64-unknown-linux-musl.tar.gz"
        );
        assert_eq!(
            archive_name("aarch64-pc-windows-msvc"),
            "anymon-aarch64-pc-windows-msvc.zip"
        );
        assert!(!TARGET.is_empty());
    }

    #[test]
    fn detects_npm_installs() {
        assert_eq!(
            package_manager(Path::new("/usr/lib/node_modules/@anymon/x/bin/anymon")),
            Some("npm")
        );
        assert_eq!(package_manager(Path::new("/usr/local/bin/anymon")), None);
    }

    #[test]
    fn hashes_with_sha256() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    fn tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        for (path, content) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, path, *content).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn zip_file(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (path, content) in entries {
            writer
                .start_file(*path, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn extracts_binaries_from_tarballs() {
        let data = tar_gz(&[
            ("anymon-x86_64-unknown-linux-musl/anymon", b"main"),
            ("anymon-x86_64-unknown-linux-musl/anymon-shell", b"shell"),
            ("anymon-x86_64-unknown-linux-musl/README.md", b"docs"),
        ]);
        let mut found = extract("anymon-x.tar.gz", &data, &["anymon", "anymon-shell"]).unwrap();
        found.sort();
        assert_eq!(
            found,
            vec![
                ("anymon".to_string(), b"main".to_vec()),
                ("anymon-shell".to_string(), b"shell".to_vec()),
            ]
        );
    }

    #[test]
    fn extracts_binaries_from_zips() {
        let data = zip_file(&[
            ("anymon-x86_64-pc-windows-msvc/anymon.exe", b"main"),
            ("anymon-x86_64-pc-windows-msvc/anymon-shell.exe", b"shell"),
        ]);
        let found = extract("anymon-x.zip", &data, &["anymon"]).unwrap();
        if cfg!(windows) {
            assert_eq!(found, vec![("anymon".to_string(), b"main".to_vec())]);
        } else {
            // `anymon.exe` only counts as the binary on Windows.
            assert!(found.is_empty());
        }
    }

    #[test]
    fn rejects_corrupt_archives() {
        assert!(extract("x.tar.gz", b"not an archive", &["anymon"]).is_err());
        assert!(extract("x.zip", b"not an archive", &["anymon"]).is_err());
    }

    #[test]
    fn replaces_files_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join(binary_name("tool"));
        std::fs::write(&target, b"old").unwrap();
        replace_file(&target, b"new").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&target).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".new"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }
}
