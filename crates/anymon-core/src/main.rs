use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use reqwest::blocking as reqwest_blocking;
use std::env;
use std::fs;
use std::io::Write;

use anymon_config::Config as AnymonConfig;
use anymon_runner::pref;

#[derive(Parser, Debug)]
#[command(name = "anymon")]
#[command(about = "Ultra-fast, language-agnostic file watcher that runs anything on change.", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path(s) to watch (overrides config watch roots)
    #[arg(long, value_name = "PATH", global = true)]
    watch: Option<Vec<String>>,

    /// Config file (TOML only)
    #[arg(long, value_name = "FILE", global = true)]
    config: Option<String>,

    /// Debounce window (ms)
    #[arg(long, value_name = "MS", global = true, default_value_t = 30)]
    debounce: u64,

    /// Kill timeout (ms)
    #[arg(long, value_name = "MS", global = true, default_value_t = 2000)]
    kill_timeout: u64,

    /// Run once and exit
    #[arg(long, global = true, default_value_t = false)]
    once: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run a command once (no shell interpretation)
    Run {
        #[arg(value_name = "COMMAND")]
        command: String,
    },
    /// Watch files based on TOML config and run tasks on change
    Watch,
    /// Debug mode (extra output)
    Debug,
    /// Update anymon to the latest version
    Update,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let config = if let Some(config_path) = &cli.config {
        if config_path.ends_with(".toml") {
            match AnymonConfig::from_toml(config_path) {
                Ok(cfg) => Some(cfg),
                Err(e) => {
                    eprintln!("{} failed to load TOML config: {e}", pref());
                    None
                }
            }
        } else {
            eprintln!(
                "{} unsupported config file format (only TOML allowed): {config_path}",
                pref()
            );
            None
        }
    } else {
        None
    };

    match &cli.command {
        Some(Commands::Run { command }) => {
            println!("{} run: {}", pref(), command);
            let mut parts = command.split_whitespace();
            if let Some(prog) = parts.next() {
                let args: Vec<&str> = parts.collect();
                match anymon_shell::run_command(prog, &args) {
                    Ok(out) => {
                        if !out.stdout.is_empty() {
                            print!("{}", out.stdout);
                        }
                        if !out.stderr.is_empty() {
                            eprint!("{}", out.stderr);
                        }
                        println!("{} process exited: {}", pref(), out.status);
                    }
                    Err(e) => eprintln!("{} failed to run '{}': {}", pref(), command, e),
                }
            } else {
                eprintln!("{} empty command", pref());
            }
        }
        Some(Commands::Watch) => {
            println!("{} watch mode", pref());
            if let Some(cfg) = config {
                anymon_runner::watch_mode(cfg, cli.watch, cli.debounce, cli.kill_timeout).await?;
            } else {
                eprintln!("{} watch requires --config anymon.toml", pref());
            }
        }
        Some(Commands::Debug) => {
            println!("{} debug mode", pref());
            if let Some(cfg) = &config {
                println!("{} loaded config: {:#?}", pref(), cfg);
            } else {
                println!("{} no config loaded", pref());
            }
        }
        Some(Commands::Update) => {
            // Move blocking update logic to a sync function and call it in a blocking context
            tokio::task::block_in_place(|| {
                if let Err(e) = update_anymon() {
                    eprintln!("{} update failed: {e}", pref());
                }
            });
        }
        None => {
            println!("{} no command specified. See --help.", pref());
        }
    }
    Ok(())
}

fn update_anymon() -> Result<()> {
    println!("{} updating anymon to the latest version...", pref());
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        eprintln!("{} unsupported OS for update", pref());
        return Ok(());
    };
    let arch = if cfg!(target_arch = "x86_64") {
        "amd64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "arm") {
        "armv7"
    } else {
        eprintln!("{} unsupported architecture for update", pref());
        return Ok(());
    };

    let repo = "builtbyjonas/anymon";
    let api_url = format!("https://api.github.com/repos/{}/releases", repo);
    let client = reqwest_blocking::Client::new();
    let releases: serde_json::Value = client
        .get(&api_url)
        .header("User-Agent", "anymon-updater")
        .send()
        .context("Failed to fetch releases info")?
        .json()
        .context("Failed to parse releases info")?;

    let releases = releases.as_array().cloned().unwrap_or_default();
    if releases.is_empty() {
        println!(
            "{} already on the latest version (no releases found)",
            pref()
        );
        return Ok(());
    }

    let latest = &releases[0];
    let latest_ver = latest["tag_name"].as_str().unwrap_or("");
    let current_ver = env!("CARGO_PKG_VERSION");
    if latest_ver.trim_start_matches('v') == current_ver {
        println!(
            "{} already on the latest version (v{})",
            pref(),
            current_ver
        );
        return Ok(());
    }

    let assets = latest["assets"].as_array().cloned().unwrap_or_default();
    let mut asset_url = None;
    let mut asset_name = None;
    for asset in &assets {
        let url = asset["browser_download_url"].as_str().unwrap_or("");
        let name = asset["name"].as_str().unwrap_or("");
        if url.contains(os) && url.contains(arch) {
            asset_url = Some(url.to_string());
            asset_name = Some(name.to_string());
            break;
        }
    }
    if asset_url.is_none() {
        eprintln!(
            "{} no prebuilt binary found for {}/{} in release {}",
            pref(),
            os,
            arch,
            latest_ver
        );
        return Ok(());
    }
    let asset_url = asset_url.unwrap();
    let asset_name = asset_name.unwrap();
    println!(
        "{} downloading {} (version {})...",
        pref(),
        asset_name,
        latest_ver
    );
    let mut resp = client
        .get(&asset_url)
        .header("User-Agent", "anymon-updater")
        .send()
        .context("Failed to download asset")?;
    let mut buf: Vec<u8> = vec![];
    resp.copy_to(&mut buf)
        .context("Failed to read asset data")?;

    let current_exe = env::current_exe().context("Failed to get current executable path")?;
    let exe_dir = current_exe
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();

    // Prefer standard per-user install directory if present (or create it):
    // Windows: %LOCALAPPDATA%\anymon
    // Linux: $XDG_DATA_HOME/anymon or $HOME/.local/share/anymon
    // macOS: $HOME/Library/Application Support/anymon
    let candidate_dir = if cfg!(target_os = "windows") {
        std::env::var("LOCALAPPDATA")
            .map(|v| std::path::PathBuf::from(v).join("anymon"))
            .unwrap_or_else(|_| exe_dir.clone())
    } else if cfg!(target_os = "linux") {
        std::env::var("XDG_DATA_HOME")
            .map(|v| std::path::PathBuf::from(v).join("anymon"))
            .unwrap_or_else(|_| {
                std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
                    .join(".local/share/anymon")
            })
    } else if cfg!(target_os = "macos") {
        std::path::PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
            .join("Library/Application Support/anymon")
    } else {
        exe_dir.clone()
    };

    let candidate_path = if cfg!(target_os = "windows") {
        candidate_dir.join("anymon.exe")
    } else {
        candidate_dir.join("anymon")
    };

    // Choose final path: prefer candidate if directory exists or can be created, otherwise fallback to exe dir
    let new_path = if candidate_dir.exists()
        || candidate_path.exists()
        || std::fs::create_dir_all(&candidate_dir).is_ok()
    {
        candidate_path
    } else if cfg!(target_os = "windows") {
        exe_dir.join("anymon.exe")
    } else {
        exe_dir.join("anymon")
    };

    let tmp_path = new_path.with_extension("tmp");
    let mut file =
        fs::File::create(&tmp_path).context("Failed to create temp file for new binary")?;
    file.write_all(&buf).context("Failed to write new binary")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = file.metadata()?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&tmp_path, perms)?;
    }
    file.sync_all()?;

    fs::rename(&tmp_path, &new_path).context("Failed to replace binary")?;
    println!("{} updated {} successfully!", pref(), new_path.display());
    Ok(())
}
