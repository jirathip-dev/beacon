use std::process::{Command, Stdio};
use std::time::Duration;

/// Writes and loads the dev.beacon.serve LaunchAgent running `beacon serve`; idempotent.
pub fn install(config: &str, host: &str, listen: &str) -> Result<String, String> {
    let home = std::env::var("HOME").map_err(|_| "install: HOME is not set")?;
    let exe = std::env::current_exe().map_err(|e| format!("install: {e}"))?;
    let config = std::fs::canonicalize(config).map_err(|e| format!("{config}: {e}"))?;
    let uid = crate::probe::output("id", &["-u"], Duration::from_secs(5)).ok_or("id -u failed")?;
    let launchctl = std::env::var("BEACON_LAUNCHCTL").unwrap_or("launchctl".into());
    let ctl = |args: &[&str]| {
        let mut cmd = Command::new(&launchctl);
        cmd.args(args)
            .stdout(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    let (domain, logs) = (
        format!("gui/{}", uid.trim()),
        format!("{home}/.local/state/beacon"),
    );
    let (path, service) = (
        format!("{home}/Library/LaunchAgents/dev.beacon.serve.plist"),
        format!("{domain}/dev.beacon.serve"),
    );
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>Label</key><string>dev.beacon.serve</string>
<key>ProgramArguments</key><array><string>{}</string><string>serve</string><string>--config</string><string>{}</string><string>--host</string><string>{host}</string><string>--listen</string><string>{listen}</string></array>
<key>EnvironmentVariables</key><dict><key>PATH</key><string>/usr/local/bin:/opt/homebrew/bin:{home}/.local/bin:{home}/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin</string></dict>
<key>RunAtLoad</key><true/><key>KeepAlive</key><true/>
<key>StandardOutPath</key><string>{logs}/serve.log</string><key>StandardErrorPath</key><string>{logs}/serve.log</string>
</dict></plist>
"#,
        exe.display(),
        config.display()
    );
    let old = std::fs::read_to_string(&path).ok();
    let loaded = ctl(&["print", &service]);
    if loaded && old.as_deref() == Some(&plist) {
        return Ok("= already dev.beacon.serve".into());
    }
    for dir in [&format!("{home}/Library/LaunchAgents"), &logs] {
        std::fs::create_dir_all(dir).map_err(|e| format!("{dir}: {e}"))?;
    }
    std::fs::write(&path, plist).map_err(|e| format!("{path}: {e}"))?;
    if loaded {
        ctl(&["bootout", &service]);
    }
    match (ctl(&["bootstrap", &domain, &path]), old) {
        (false, _) => Err(format!(
            "install: launchctl bootstrap {domain} {path} failed"
        )),
        (true, Some(_)) => Ok("~ updated dev.beacon.serve".into()),
        (true, None) => Ok("+ installed dev.beacon.serve".into()),
    }
}
