use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[cfg(target_os = "macos")]
const LAUNCHCTL: (&str, &str) = ("BEACON_LAUNCHCTL", "launchctl");
const HOSTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/hosts");

fn install(home: &Path, listen: &str, (var, ctl): (&str, &str)) -> Output {
    let config = r#"{ "version": 1, "monitors": [], "hosts": { "box": { "url": "https://box/status", "checks": [] } } }"#;
    std::fs::write(home.join("beacon.json"), config).unwrap();
    Command::new(env!("CARGO_BIN_EXE_beacon"))
        .args(["install", "--config", "beacon.json", "--host", "box"])
        .args(["--listen", listen])
        .current_dir(home)
        .env("HOME", home)
        .env(var, home.join(ctl))
        .output()
        .unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let home = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::remove_dir_all(&home).ok();
    std::fs::create_dir_all(&home).unwrap();
    home
}

#[cfg(target_os = "macos")]
fn stdout(o: &Output) -> String {
    assert!(o.status.success(), "{o:?}");
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[cfg(target_os = "macos")]
#[test]
fn install_writes_loads_then_is_idempotent_and_reloads_on_change() {
    use std::os::unix::fs::PermissionsExt;
    let home = scratch("install-home");
    let fake = "#!/bin/sh\necho \"$*\" >> \"$HOME/calls\"\ncase $1 in print) test -f \"$HOME/loaded\" ;; bootstrap) touch \"$HOME/loaded\" ;; bootout) rm \"$HOME/loaded\" ;; esac\n";
    std::fs::write(home.join("launchctl"), fake).unwrap();
    std::fs::set_permissions(home.join("launchctl"), PermissionsExt::from_mode(0o755)).unwrap();
    let calls = || std::fs::read_to_string(home.join("calls")).unwrap_or_default();
    let reset = || std::fs::remove_file(home.join("calls")).unwrap();
    let plist_path = home.join("Library/LaunchAgents/dev.beacon.serve.plist");
    let uid = String::from_utf8(Command::new("id").arg("-u").output().unwrap().stdout).unwrap();
    let gui = format!("gui/{}", uid.trim());

    assert_eq!(
        stdout(&install(&home, "127.0.0.1:7878", LAUNCHCTL)),
        "+ installed dev.beacon.serve\n"
    );
    let plist = std::fs::read_to_string(&plist_path).unwrap();
    let (exe, config) = (env!("CARGO_BIN_EXE_beacon"), home.join("beacon.json"));
    let config = std::fs::canonicalize(config).unwrap();
    let args = format!(
        "<key>ProgramArguments</key><array><string>{exe}</string><string>serve</string><string>--config</string><string>{}</string><string>--host</string><string>box</string><string>--listen</string><string>127.0.0.1:7878</string></array>",
        config.display()
    );
    assert!(Path::new(exe).is_absolute() && config.is_absolute());
    let h = home.display();
    for want in [
        args,
        format!(
            "<key>PATH</key><string>/usr/local/bin:/opt/homebrew/bin:{h}/.local/bin:{h}/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>"
        ),
        "<key>RunAtLoad</key><true/><key>KeepAlive</key><true/>".into(),
        format!(
            "<key>StandardOutPath</key><string>{h}/.local/state/beacon/serve.log</string><key>StandardErrorPath</key><string>{h}/.local/state/beacon/serve.log</string>"
        ),
    ] {
        assert!(plist.contains(&want), "missing {want}\nin {plist}");
    }
    let lint = Command::new("plutil")
        .arg("-lint")
        .arg(&plist_path)
        .output()
        .unwrap();
    assert!(lint.status.success(), "{lint:?}");
    assert!(home.join(".local/state/beacon").is_dir());
    let p = plist_path.display();
    assert_eq!(
        calls(),
        format!("print {gui}/dev.beacon.serve\nbootstrap {gui} {p}\n")
    );

    reset();
    assert_eq!(
        stdout(&install(&home, "127.0.0.1:7878", LAUNCHCTL)),
        "= already dev.beacon.serve\n"
    );
    assert_eq!(calls(), format!("print {gui}/dev.beacon.serve\n"));
    assert_eq!(std::fs::read_to_string(&plist_path).unwrap(), plist);

    reset();
    assert_eq!(
        stdout(&install(&home, "0.0.0.0:9000", LAUNCHCTL)),
        "~ updated dev.beacon.serve\n"
    );
    assert!(
        std::fs::read_to_string(&plist_path)
            .unwrap()
            .contains("<string>0.0.0.0:9000</string>")
    );
    let reload = format!(
        "print {gui}/dev.beacon.serve\nbootout {gui}/dev.beacon.serve\nbootstrap {gui} {p}\n"
    );
    assert_eq!(calls(), reload);
}

#[cfg(target_os = "linux")]
#[test]
fn install_writes_and_enables_the_unit_then_is_idempotent_and_restarts_on_change() {
    use std::os::unix::fs::PermissionsExt;
    let home = scratch("install-linux");
    std::fs::write(
        home.join("systemctl"),
        "#!/bin/sh\necho \"$*\" >> \"$HOME/calls\"\n",
    )
    .unwrap();
    std::fs::set_permissions(home.join("systemctl"), PermissionsExt::from_mode(0o755)).unwrap();
    let run = |listen: &str| {
        let o = install(&home, listen, ("BEACON_SYSTEMCTL", "systemctl"));
        assert!(o.status.success(), "{o:?}");
        let calls = std::fs::read_to_string(home.join("calls")).unwrap_or_default();
        std::fs::remove_file(home.join("calls")).ok();
        (String::from_utf8_lossy(&o.stdout).into_owned(), calls)
    };
    let unit_path = home.join(".config/systemd/user/beacon-serve.service");
    let enable = "--user daemon-reload\n--user enable --now beacon-serve.service\n";

    assert_eq!(
        run("127.0.0.1:7878"),
        ("+ installed beacon-serve.service\n".into(), enable.into())
    );
    let unit = std::fs::read_to_string(&unit_path).unwrap();
    let config = std::fs::canonicalize(home.join("beacon.json")).unwrap();
    let h = home.display();
    for want in [
        format!(
            "\nExecStart=\"{}\" serve --config \"{}\" --host box --listen 127.0.0.1:7878\n",
            env!("CARGO_BIN_EXE_beacon"),
            config.display()
        ),
        format!(
            "\nEnvironment=PATH=/usr/local/bin:/opt/homebrew/bin:{h}/.local/bin:{h}/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin\n"
        ),
        "\nRestart=always\n".into(),
        format!("\nStandardOutput=append:{h}/.local/state/beacon/serve.log\n"),
        "\n[Install]\nWantedBy=default.target\n".into(),
    ] {
        assert!(unit.contains(&want), "missing {want}\nin {unit}");
    }
    assert!(home.join(".local/state/beacon").is_dir());

    assert_eq!(
        run("127.0.0.1:7878"),
        ("= already beacon-serve.service\n".into(), String::new())
    );
    assert_eq!(std::fs::read_to_string(&unit_path).unwrap(), unit);

    let restart = "--user daemon-reload\n--user restart beacon-serve.service\n";
    assert_eq!(
        run("0.0.0.0:9000"),
        ("~ updated beacon-serve.service\n".into(), restart.into())
    );
    let unit = std::fs::read_to_string(&unit_path).unwrap();
    assert!(unit.contains("--listen 0.0.0.0:9000\n"), "{unit}");
}

#[test]
fn mini_config_is_valid_and_every_check_is_valid_sh() {
    host_config_is_valid("mini.json", "jirathips-mac-mini", "Mini", 12);
}

#[test]
fn boat_config_is_valid_and_every_check_is_valid_sh() {
    host_config_is_valid("boat.json", "boat-dev-1", "Boat", 3);
}

fn host_config_is_valid(file: &str, host: &str, group: &str, n: usize) {
    let path = format!("{HOSTS}/{file}");
    let o = Command::new(env!("CARGO_BIN_EXE_beacon"))
        .args(["check", &path])
        .output()
        .unwrap();
    assert!(o.status.success(), "{o:?}");
    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let checks = config["hosts"][host]["checks"].as_array().unwrap();
    let ids: Vec<_> = checks.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), n, "{ids:?}");
    for c in checks {
        let sh = Command::new("sh")
            .args(["-n", "-c", c["command"].as_str().unwrap()])
            .output()
            .unwrap();
        assert!(sh.status.success(), "{}: {sh:?}", c["id"]);
    }
    let monitored: Vec<_> = config["monitors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["group"] == group && m["host"] == host)
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(monitored, ids);
}
