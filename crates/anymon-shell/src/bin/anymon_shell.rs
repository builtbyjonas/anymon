//! `anymon-shell <program> [args...]` runs a program directly, without an
//! intermediate shell, and exits with the program's exit code.

use std::process::{exit, Command};

fn main() {
    let mut args = std::env::args_os().skip(1);
    let Some(program) = args.next() else {
        eprintln!("Usage: anymon-shell <program> [args...]");
        eprintln!("Runs the program directly (no shell) and exits with its exit code.");
        exit(2);
    };
    if program == "--version" || program == "-V" {
        println!("anymon-shell {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    match Command::new(&program).args(args).status() {
        Ok(status) => exit(anymon_shell::exit_code(&status)),
        Err(err) => {
            eprintln!(
                "anymon-shell: failed to run '{}': {err}",
                program.to_string_lossy()
            );
            exit(if err.kind() == std::io::ErrorKind::NotFound {
                127
            } else {
                126
            });
        }
    }
}
