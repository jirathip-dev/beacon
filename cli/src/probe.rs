use crate::status::Status;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const DRAIN: Duration = Duration::from_millis(100);

/// Runs `sh -c command`; up on exit 0. Past `timeout` its whole process group is killed.
pub fn run(command: &str, timeout: Duration) -> (Status, String) {
    let mut sh = Command::new("sh");
    sh.args(["-c", command]);
    match spawn_and_wait(sh, timeout, true) {
        Ok(Some((ok, output))) => (
            if ok { Status::Up } else { Status::Down },
            last_line(&output),
        ),
        Ok(None) => (
            Status::Down,
            format!("timeout after {}s", timeout.as_secs()),
        ),
        Err(e) => (Status::Down, cut(&format!("spawn failed: {e}"))),
    }
}

/// Runs `program` with `args`, no shell; its output on exit 0 within `timeout`, else `None`.
pub fn output(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    match spawn_and_wait(cmd, timeout, false) {
        Ok(Some((true, output))) => Some(output),
        _ => None,
    }
}

fn spawn_and_wait(
    mut cmd: Command,
    timeout: Duration,
    reap: bool,
) -> std::io::Result<Option<(bool, String)>> {
    let (mut reader, writer) = std::io::pipe()?;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(writer.try_clone()?)
        .stderr(writer)
        .process_group(0)
        .spawn()?;
    drop(cmd);
    let pgid = child.id();
    let (out_tx, output) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).ok();
        out_tx.send(String::from_utf8_lossy(&buf).into_owned())
    });
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || tx.send(child.wait()));
    let Ok(exit) = rx.recv_timeout(timeout) else {
        kill_group(pgid);
        rx.recv().ok();
        output.recv().ok();
        return Ok(None);
    };
    let exit = exit?;
    let drained = (!reap).then(|| output.recv_timeout(DRAIN).ok()).flatten();
    let text = drained.unwrap_or_else(|| {
        kill_group(pgid);
        output.recv().unwrap_or_default()
    });
    Ok(Some((exit.success(), text)))
}

fn kill_group(pgid: u32) {
    Command::new("kill")
        .args(["-KILL", "--", &format!("-{pgid}")])
        .stderr(Stdio::null())
        .status()
        .ok();
}

fn last_line(output: &str) -> String {
    cut(output
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or(""))
}

fn cut(s: &str) -> String {
    s.chars().take(200).collect()
}

/// The short hostname, lower case: a host's default name in the config.
pub fn hostname() -> Result<String, String> {
    let out = Command::new("hostname")
        .arg("-s")
        .output()
        .map_err(|e| format!("hostname: {e}"))?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_lowercase();
    (out.status.success() && !name.is_empty())
        .then_some(name)
        .ok_or("hostname -s failed; pass --host <name>".into())
}
