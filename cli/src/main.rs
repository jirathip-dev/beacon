use beacon::config::{self, Host, Id, MonitorsConfig};
use std::process::{Command, ExitCode};

const USAGE: &str = "usage: beacon check <path>
       beacon run [--config <path>] [--host <name>]
       beacon serve [--config <path>] [--host <name>] [--listen <addr>]
       beacon install [--config <path>] [--host <name>] [--listen <addr>]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["check", path] => load(path).map(|_| ExitCode::SUCCESS),
        ["run", opts @ ..] => {
            target(opts, false).map(|(_, host, _, _)| match beacon::run::round(&host) {
                true => ExitCode::SUCCESS,
                false => ExitCode::from(1),
            })
        }
        ["serve", opts @ ..] => target(opts, true)
            .and_then(|(name, _, listen, path)| Err(beacon::serve::serve(&name, &path, &listen))),
        ["install", opts @ ..] => target(opts, true).and_then(|(name, _, listen, path)| {
            beacon::install::install(&path, &name, &listen).map(|done| {
                println!("{done}");
                ExitCode::SUCCESS
            })
        }),
        _ => Err(USAGE.into()),
    };
    result.unwrap_or_else(|e| {
        eprintln!("{e}");
        ExitCode::from(2)
    })
}

fn target(opts: &[&str], serve: bool) -> Result<(String, Host, String, String), String> {
    let (mut path, mut host) = (std::env::var("BEACON_CONFIG").ok(), None);
    let mut listen = "127.0.0.1:7878".to_string();
    for pair in opts.chunks(2) {
        match pair {
            ["--config", v] => path = Some(v.to_string()),
            ["--host", v] => host = Some(v.to_string()),
            ["--listen", v] if serve => listen = v.to_string(),
            _ => return Err(USAGE.into()),
        }
    }
    let path = path.ok_or("no config: pass --config <path> or set BEACON_CONFIG")?;
    let mut config = load(&path)?;
    let host = match host {
        Some(h) => h,
        None => short_hostname()?,
    };
    let checks = Id::try_from(host.clone())
        .ok()
        .and_then(|id| config.hosts.remove(&id))
        .ok_or(format!("{path}: no host {host:?} in hosts"))?;
    Ok((host, checks, listen, path))
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
