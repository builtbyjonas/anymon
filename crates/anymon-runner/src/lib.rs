//! File watching and task supervision for anymon.
//!
//! 1. A [`Config`](anymon_config::Config) is resolved into a [`Plan`]: commands
//!    are parsed, glob patterns compiled and paths resolved.
//! 2. [`watch`] registers file-system watches for the directories the
//!    patterns need, filters events (user ignores, built-in ignores,
//!    `.gitignore`), debounces them per task and restarts the affected tasks.
//! 3. [`run_once`] runs every task a single time, for scripts and CI.
//!
//! Every task runs in its own process group (job object on Windows), so a
//! restart stops the complete process tree and never leaves orphans behind.

mod ignore;
pub mod paths;
pub mod pattern;
mod plan;
mod session;
mod task;
pub mod ui;
mod watcher;

pub use plan::{Plan, PlanOptions, TaskPlan};
pub use session::{run_once, watch, Reloader};
