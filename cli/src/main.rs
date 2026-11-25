use beacon::cmd;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let found = args
        .split_first()
        .and_then(|(name, rest)| Some((cmd::ALL.iter().find(|c| c.name == *name)?, rest)));
    let result = found.map_or_else(|| Err(cmd::usage()), |(c, rest)| (c.run)(rest));
    result.unwrap_or_else(|e| {
        beacon::out::err(&e);
        ExitCode::from(2)
    })
}
