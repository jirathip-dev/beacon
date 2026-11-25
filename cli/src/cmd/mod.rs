pub mod check;
pub mod install;
pub mod jobs;
pub mod run;
pub mod serve;

use std::process::ExitCode;

/// One `beacon <name>` command: its usage line and its entry point.
pub struct Command {
    pub name: &'static str,
    pub usage: &'static str,
    pub run: fn(&[&str]) -> Result<ExitCode, String>,
}

#[rustfmt::skip]
pub const ALL: &[Command] = &[
    Command { name: "check", usage: check::USAGE, run: check::run },
    Command { name: "install", usage: install::USAGE, run: install::run },
    Command { name: "jobs", usage: jobs::USAGE, run: jobs::run },
    Command { name: "run", usage: run::USAGE, run: run::run },
    Command { name: "serve", usage: serve::USAGE, run: serve::run },
];

/// Every command's usage line, for an unknown or missing command.
pub fn usage() -> String {
    let lines: Vec<_> = ALL.iter().map(|c| c.usage).collect();
    format!("usage: {}", lines.join("\n       "))
}
