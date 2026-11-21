use crate::config::{self, Id, Kept, Label, Review};
use crate::jobs::{Manager, Watcher, edit};
use crate::out;
use crate::target::Target;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

pub const USAGE: &str = "beacon jobs [keep <label> --name <n> --group <g> [--running] | ignore <label> | removed <label>] [--config <path>] [--host <name>]";

/// `beacon jobs`: lists this host's jobs with their review, or sets one job's review in the config.
pub fn run(args: &[&str]) -> Result<ExitCode, String> {
    let usage = || format!("usage: {USAGE}");
    let (verb, label, rest) = match args {
        [v @ ("keep" | "ignore" | "removed"), label, rest @ ..] => (Some(*v), *label, rest),
        rest => (None, "", rest),
    };
    let (Target { path, name: host }, rest) = Target::take(rest)?;
    let (mut name, mut group, mut running) = (None, None, false);
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().map(ToString::to_string).ok_or_else(usage);
        match *flag {
            "--name" if verb == Some("keep") => name = Some(value()?),
            "--group" if verb == Some("keep") => group = Some(value()?),
            "--running" if verb == Some("keep") => running = true,
            _ => return Err(usage()),
        }
    }
    let id = Id::try_from(host.clone())?;
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let review = match verb {
        None => return list(&text, &id).map_err(|e| format!("{path}: {e}")),
        Some("keep") => {
            let (name, group) = (name.ok_or_else(usage)?, group.ok_or_else(usage)?);
            Review::Keep(Kept {
                name,
                group,
                running,
            })
        }
        Some("ignore") => Review::Ignore,
        _ => Review::Removed,
    };
    let label = Label::try_from(label.to_string())?;
    let new = edit::set(&text, &id, &label, &review).map_err(|e| format!("{path}: {e}"))?;
    let verb = verb.unwrap_or_default();
    let mut what = format!("{verb} {}", label.as_str());
    if let Review::Keep(k) = &review {
        what += &format!(" as {:?} in {}", k.name, k.group);
    }
    if new == text {
        out::out(&format!("= {host}: {what}"));
        return Ok(ExitCode::SUCCESS);
    }
    std::fs::write(&path, new).map_err(|e| format!("{path}: {e}"))?;
    out::out(&format!("~ {host}: {what}"));
    let (dir, file) = (Path::new(&path).parent(), Path::new(&path).file_name());
    let dir = dir
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let (dir, file) = (dir.display(), file.unwrap_or_default().display());
    out::out(&format!(
        "git -C {dir} commit -m \"beacon jobs: {verb} {}\" {file} && git -C {dir} push",
        label.as_str()
    ));
    Ok(ExitCode::SUCCESS)
}

fn list(text: &str, id: &Id) -> Result<ExitCode, String> {
    let mut config = config::parse(text)?;
    let host = config
        .hosts
        .remove(id)
        .ok_or(format!("no host {:?} in hosts", id.as_str()))?;
    let view = host.jobs.iter().map(|(l, r)| match r {
        Review::Ignore => (l.clone(), Review::Removed),
        r => (l.clone(), r.clone()),
    });
    let manager = Manager::here();
    let listing = manager.list().ok_or("jobs: listing failed")?;
    let rows = Watcher::default().update(&listing, &view.collect(), Instant::now(), |l| {
        manager.read(l)
    });
    let mut lines: Vec<[String; 5]> = rows
        .iter()
        .map(|r| {
            let (state, names) = match host.jobs.get(r.label.as_str()) {
                Some(Review::Keep(k)) => ("kept", format!("{:?} in {}", k.name, k.group)),
                Some(Review::Ignore) => ("ignored", String::new()),
                _ => ("unreviewed", String::new()),
            };
            [
                state,
                r.status.to_string().as_str(),
                &r.label,
                &r.message,
                &names,
            ]
            .map(String::from)
        })
        .collect();
    for (label, review) in &host.jobs {
        let state = match review {
            Review::Ignore => "ignored",
            Review::Removed => "removed",
            Review::Keep(_) => continue,
        };
        if !listing.iter().any(|j| j.label == label.as_str()) {
            lines.push([state, "-", label.as_str(), "not loaded", ""].map(String::from));
        }
    }
    let width = |i: usize| {
        lines
            .iter()
            .filter_map(|l| l.get(i))
            .map(|c| c.chars().count())
            .max()
    };
    let widths: Vec<_> = (0..5).map(|i| width(i).unwrap_or(0)).collect();
    for line in &lines {
        let cells: Vec<_> = line
            .iter()
            .zip(&widths)
            .map(|(c, w)| format!("{c:w$}"))
            .collect();
        out::out(cells.join("  ").trim_end());
    }
    Ok(ExitCode::SUCCESS)
}
