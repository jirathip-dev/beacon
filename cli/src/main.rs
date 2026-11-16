use beacon::config::{self, Id, MonitorsConfig};
use std::process::{Command, ExitCode};

const USAGE: &str =
    "usage: beacon check <path>\n       beacon run [--config <path>] [--host <name>]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["check", path] => load(path).map(|_| ExitCode::SUCCESS),
        ["run", opts @ ..] => run(opts),
        _ => Err(USAGE.into()),
    };
    result.unwrap_or_else(|e| {
        eprintln!("{e}");
        ExitCode::from(2)
    })
}

fn run(opts: &[&str]) -> Result<ExitCode, String> {
    let (mut path, mut host) = (std::env::var("BEACON_CONFIG").ok(), None);
    for pair in opts.chunks(2) {
        match pair {
            ["--config", v] => path = Some(v.to_string()),
            ["--host", v] => host = Some(v.to_string()),
            _ => return Err(USAGE.into()),
        }
    }
    let path = path.ok_or("no config: pass --config <path> or set BEACON_CONFIG")?;
    let config = load(&path)?;
    let host = match host {
        Some(h) => h,
        None => short_hostname()?,
    };
    let checks = Id::try_from(host.clone())
        .ok()
        .and_then(|id| config.hosts.get(&id))
        .ok_or(format!("{path}: no host {host:?} in hosts"))?;
    Ok(if beacon::run::round(checks) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn load(path: &str) -> Result<MonitorsConfig, String> {
    std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|text| config::parse(&text))
        .map_err(|e| format!("{path}: {e}"))
}

fn short_hostname() -> Result<String, String> {
    let out = Command::new("hostname")
        .arg("-s")
        .output()
        .map_err(|e| format!("hostname: {e}"))?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_lowercase();
    (out.status.success() && !name.is_empty())
        .then_some(name)
        .ok_or("hostname -s failed; pass --host <name>".into())
}
