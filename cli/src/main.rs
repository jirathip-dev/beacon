use beacon::config::{self, Host, Id, MonitorsConfig};
use beacon::notify::Notify;
use std::process::ExitCode;

const USAGE: &str = "usage: beacon check <path>
       beacon run [--config <path>] [--host <name>]
       beacon serve [--config <path>] [--host <name>] [--listen <addr>] [--notify <path>]
       beacon install [--config <path>] [--host <name>] [--listen <addr>] [--notify <path>]
       beacon jobs [keep <label> --name <n> --group <g> [--running] | ignore <label> | removed <label>]
                   [--config <path>] [--host <name>]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["check", path] => load(path).map(|_| ExitCode::SUCCESS),
        ["run", opts @ ..] => target(opts, false).map(|(_, host, _, _, _)| {
            if beacon::run::round(&host) {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }),
        ["serve", opts @ ..] => target(opts, true).and_then(|(name, _, listen, path, notify)| {
            let notify = notify.as_deref().map(Notify::load).transpose()?;
            Err(beacon::serve::serve(&name, &path, &listen, notify))
        }),
        ["install", opts @ ..] => target(opts, true).and_then(|(name, _, listen, path, notify)| {
            beacon::install::install(&path, &name, &listen, notify.as_deref()).map(|done| {
                beacon::out::out(&done);
                ExitCode::SUCCESS
            })
        }),
        ["jobs", rest @ ..] => beacon::jobs::cli::run(rest).map(|()| ExitCode::SUCCESS),
        _ => Err(USAGE.into()),
    };
    result.unwrap_or_else(|e| {
        beacon::out::err(&e);
        ExitCode::from(2)
    })
}

type Target = (String, Host, String, String, Option<String>);

fn target(opts: &[&str], serve: bool) -> Result<Target, String> {
    let (mut path, mut host, mut notify) = (std::env::var("BEACON_CONFIG").ok(), None, None);
    let mut listen = "127.0.0.1:7878".to_string();
    for pair in opts.chunks(2) {
        match pair {
            ["--config", v] => path = Some(v.to_string()),
            ["--host", v] => host = Some(v.to_string()),
            ["--listen", v] if serve => listen = v.to_string(),
            ["--notify", v] if serve => notify = Some(v.to_string()),
            _ => return Err(USAGE.into()),
        }
    }
    let path = path.ok_or("no config: pass --config <path> or set BEACON_CONFIG")?;
    let mut config = load(&path)?;
    let host = match host {
        Some(h) => h,
        None => beacon::probe::hostname()?,
    };
    let checks = Id::try_from(host.clone())
        .ok()
        .and_then(|id| config.hosts.remove(&id))
        .ok_or(format!("{path}: no host {host:?} in hosts"))?;
    Ok((host, checks, listen, path, notify))
}

fn load(path: &str) -> Result<MonitorsConfig, String> {
    std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|text| config::parse(&text))
        .map_err(|e| format!("{path}: {e}"))
}
