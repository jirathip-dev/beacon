use crate::target::Target;
use std::process::ExitCode;

pub const USAGE: &str = "beacon run [--config <path>] [--host <name>]";

/// `beacon run`: one round of this host's checks; exit 1 when one is down.
pub fn run(args: &[&str]) -> Result<ExitCode, String> {
    let (target, rest) = Target::take(args)?;
    if !rest.is_empty() {
        return Err(format!("usage: {USAGE}"));
    }
    Ok(if crate::run::round(&target.host()?) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}
