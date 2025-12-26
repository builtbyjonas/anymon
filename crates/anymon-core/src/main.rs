use anyhow::Result;
use clap::{Parser, Subcommand};

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
            // Run the command directly without invoking an external shell.
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
        None => {
            println!("{} no command specified. See --help.", pref());
        }
    }
    Ok(())
}
