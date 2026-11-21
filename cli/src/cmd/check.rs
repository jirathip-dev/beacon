use crate::target;
use std::process::ExitCode;

pub const USAGE: &str = "beacon check <path>";

/// `beacon check`: validates a config file.
pub fn run(args: &[&str]) -> Result<ExitCode, String> {
    match args {
        [path] => target::load(path).map(|_| ExitCode::SUCCESS),
        _ => Err(format!("usage: {USAGE}")),
    }
}
