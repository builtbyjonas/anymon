use std::env;
use std::process::exit;

fn main() {
    let mut args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: anymon-shell <command> [args...]");
        eprintln!(
            "Runs the given command directly without invoking an external interactive shell."
        );
        exit(2);
    }

    // first arg is program name
    let cmd = args.remove(1);
    let cmd_args: Vec<String> = args.into_iter().skip(1).collect();

    match anymon_shell::run_command(cmd.clone(), &cmd_args) {
        Ok(out) => {
            if !out.stdout.is_empty() {
                print!("{}", out.stdout);
            }
            if !out.stderr.is_empty() {
                eprint!("{}", out.stderr);
            }
            exit(out.status);
        }
        Err(e) => {
            eprintln!("failed to run '{}': {}", cmd, e);
            exit(1);
        }
    }
}
