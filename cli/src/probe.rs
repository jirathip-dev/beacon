use crate::status::Status;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// Runs `sh -c command`; up on exit 0. Past `timeout` its whole process group is killed.
pub fn run(command: &str, timeout: Duration) -> (Status, String) {
    match spawn_and_wait(command, timeout) {
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

fn spawn_and_wait(command: &str, timeout: Duration) -> std::io::Result<Option<(bool, String)>> {
    let (mut reader, writer) = std::io::pipe()?;
    let mut child = Command::new("sh")
        .args(["-c", command])
        .stdin(Stdio::null())
        .stdout(writer.try_clone()?)
        .stderr(writer)
        .process_group(0)
        .spawn()?;
    let pgid = child.id();
    let output = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = reader.read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    });
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || tx.send(child.wait()));
    let exit = match rx.recv_timeout(timeout) {
        Ok(exit) => exit?,
        Err(_) => {
            kill_group(pgid);
            let _ = rx.recv();
            let _ = output.join();
            return Ok(None);
        }
    };
    kill_group(pgid);
    Ok(Some((exit.success(), output.join().unwrap_or_default())))
}

fn kill_group(pgid: u32) {
    let _ = Command::new("kill")
        .args(["-KILL", "--", &format!("-{pgid}")])
        .stderr(Stdio::null())
        .status();
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
