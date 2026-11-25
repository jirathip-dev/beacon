use std::process::{Command, Stdio};
use std::time::Duration;

/// Installs `beacon <serve>` as `LaunchAgent` `dev.beacon.serve` (macOS) or systemd user unit `beacon-serve.service`; idempotent.
pub fn install(serve: &[String]) -> Result<String, String> {
    let home = std::env::var("HOME").map_err(|_| "install: HOME is not set")?;
    let exe = std::env::current_exe().map_err(|e| format!("install: {e}"))?;
    let argv: Vec<String> = [exe.display().to_string()]
        .into_iter()
        .chain(serve.iter().cloned())
        .collect();
    let plain = |a: &str| {
        a.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:-".contains(c))
    };
    let quote = |a: &String| {
        if plain(a) {
            a.clone()
        } else {
            format!("\"{a}\"")
        }
    };
    let exec = argv.iter().map(quote).collect::<Vec<_>>().join(" ");
    let strings: String = argv
        .iter()
        .map(|a| format!("<string>{a}</string>"))
        .collect();
    let logs = crate::state::dir(std::path::Path::new(&home));
    let logs = logs.to_string_lossy();
    let path = format!(
        "/usr/local/bin:/opt/homebrew/bin:{home}/.local/bin:{home}/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin"
    );
    if !cfg!(target_os = "macos") {
        let unit = format!(
            "[Unit]\nDescription=beacon serve\n\n[Service]\nExecStart={exec}\nEnvironment=PATH={path}\nRestart=always\nStandardOutput=append:{logs}/serve.log\nStandardError=append:{logs}/serve.log\n\n[Install]\nWantedBy=default.target\n"
        );
        let dir = format!("{home}/.config/systemd/user");
        let file = format!("{dir}/beacon-serve.service");
        let old = std::fs::read_to_string(&file).ok();
        if old.as_deref() == Some(&unit) {
            return Ok("= already beacon-serve.service".into());
        }
        write(&[&dir, &logs], &file, &unit)?;
        let start: &[&str] = match old {
            Some(_) => &["--user", "restart", "beacon-serve.service"],
            None => &["--user", "enable", "--now", "beacon-serve.service"],
        };
        let systemctl = std::env::var("BEACON_SYSTEMCTL").unwrap_or("systemctl".into());
        let ok = ctl(&systemctl, &["--user", "daemon-reload"]) && ctl(&systemctl, start);
        let step = format!("{systemctl} {}", start.join(" "));
        return done(ok, old, "beacon-serve.service", &step);
    }
    let uid = crate::probe::output("id", &["-u"], Duration::from_secs(5)).ok_or("id -u failed")?;
    let launchctl = std::env::var("BEACON_LAUNCHCTL").unwrap_or("launchctl".into());
    let domain = format!("gui/{}", uid.trim());
    let (file, service) = (
        format!("{home}/Library/LaunchAgents/dev.beacon.serve.plist"),
        format!("{domain}/dev.beacon.serve"),
    );
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>Label</key><string>dev.beacon.serve</string>
<key>ProgramArguments</key><array>{strings}</array>
<key>EnvironmentVariables</key><dict><key>PATH</key><string>{path}</string></dict>
<key>RunAtLoad</key><true/><key>KeepAlive</key><true/>
<key>StandardOutPath</key><string>{logs}/serve.log</string><key>StandardErrorPath</key><string>{logs}/serve.log</string>
</dict></plist>
"#
    );
    let old = std::fs::read_to_string(&file).ok();
    let loaded = ctl(&launchctl, &["print", &service]);
    if loaded && old.as_deref() == Some(&plist) {
        return Ok("= already dev.beacon.serve".into());
    }
    write(
        &[&format!("{home}/Library/LaunchAgents"), &logs],
        &file,
        &plist,
    )?;
    if loaded {
        ctl(&launchctl, &["bootout", &service]);
    }
    let ok = ctl(&launchctl, &["bootstrap", &domain, &file]);
    let step = format!("launchctl bootstrap {domain} {file}");
    done(ok, old, "dev.beacon.serve", &step)
}

fn ctl(program: &str, args: &[&str]) -> bool {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdout(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn write(dirs: &[&str], file: &str, text: &str) -> Result<(), String> {
    for dir in dirs {
        std::fs::create_dir_all(dir).map_err(|e| format!("{dir}: {e}"))?;
    }
    std::fs::write(file, text).map_err(|e| format!("{file}: {e}"))
}

fn done(ok: bool, old: Option<String>, name: &str, step: &str) -> Result<String, String> {
    match (ok, old) {
        (false, _) => Err(format!("install: {step} failed")),
        (true, Some(_)) => Ok(format!("~ updated {name}")),
        (true, None) => Ok(format!("+ installed {name}")),
    }
}
