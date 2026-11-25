use crate::cmd::serve::ServeArgs;
use crate::out;
use std::process::ExitCode;

pub const USAGE: &str = "beacon install [<beacon serve's flags>]";

/// `beacon install`: installs `beacon serve` with these flags as a user service.
pub fn run(args: &[&str]) -> Result<ExitCode, String> {
    let serve = ServeArgs::parse(args, USAGE)?.argv()?;
    out::out(&crate::install::install(&serve)?);
    Ok(ExitCode::SUCCESS)
}
