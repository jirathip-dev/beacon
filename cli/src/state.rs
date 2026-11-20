use crate::history::KEEP_S;
use crate::out;
use crate::serve::over_budget;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const LOG_ROTATE_BYTES: u64 = 256 << 10;
const STATE_CAP_BYTES: u64 = 20 << 20;

/// Serve's state folder under `home`: `serve.log`, its one rotation and later stores.
pub fn dir(home: &Path) -> PathBuf {
    home.join(".local/state/beacon")
}

/// Rotates `serve.log` past 256 KB to `serve.log.1` and trims the folder to 20 MB, oldest files first.
pub fn tidy(dir: &Path) {
    if !over_budget("rotate") {
        rotate(dir).ok();
    }
    if !over_budget("trim") {
        trim(dir);
    }
    let old = SystemTime::now() - Duration::from_secs(KEEP_S);
    for e in fs::read_dir(times(dir)).into_iter().flatten().flatten() {
        if e.metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|t| t < old)
        {
            fs::remove_file(e.path()).ok();
        }
    }
}

/// The folder of the prober's ring files.
pub fn times(dir: &Path) -> PathBuf {
    dir.join("history/times")
}

/// `events.json`'s bytes and its modification time in Unix seconds.
pub fn events(dir: &Path) -> Option<(Vec<u8>, u64)> {
    let path = dir.join("history/events.json");
    let at = fs::metadata(&path).and_then(|m| m.modified()).ok()?;
    let at = at.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    Some((fs::read(&path).ok()?, at))
}

/// Writes `bytes` to `events.json` (a temporary file, then rename) when `changed`, else sets its modification time to now.
pub fn keep_events(dir: &Path, bytes: &[u8], changed: bool) -> std::io::Result<()> {
    let path = dir.join("history/events.json");
    if !changed {
        let file = fs::File::options().write(true).open(&path)?;
        return file.set_modified(SystemTime::now());
    }
    fs::create_dir_all(times(dir))?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, &path)
}

fn rotate(dir: &Path) -> std::io::Result<()> {
    let log = dir.join("serve.log");
    let len = fs::metadata(&log)?.len();
    if len <= LOG_ROTATE_BYTES {
        return Ok(());
    }
    let mut tail = Vec::new();
    let mut file = fs::File::open(&log)?;
    file.seek(SeekFrom::Start(len.saturating_sub(LOG_ROTATE_BYTES)))?;
    file.take(LOG_ROTATE_BYTES).read_to_end(&mut tail)?;
    let tmp = dir.join("serve.log.1.tmp");
    fs::write(&tmp, tail)?;
    fs::rename(&tmp, dir.join("serve.log.1"))?;
    fs::OpenOptions::new().write(true).open(&log)?.set_len(0)
}

fn trim(dir: &Path) {
    let mut files = Vec::new();
    walk(dir, &mut files);
    let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
    files.sort();
    for (_, len, path) in files {
        if total <= STATE_CAP_BYTES {
            break;
        }
        let kept =
            path.ends_with("serve.log") || path.parent().is_some_and(|p| p.ends_with("jobs"));
        if !kept && fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

fn walk(dir: &Path, files: &mut Vec<(SystemTime, u64, PathBuf)>) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            walk(&entry.path(), files);
        } else {
            let at = meta.modified().unwrap_or(UNIX_EPOCH);
            files.push((at, meta.len(), entry.path()));
        }
    }
}

#[derive(Deserialize)]
struct AnnouncedFile {
    version: u8,
    announced: BTreeSet<String>,
}

/// The job labels a host already announced, in `jobs/<host>.json` (ADR D12 §6).
pub struct Announced {
    path: PathBuf,
    labels: BTreeSet<String>,
    baseline: bool,
}

impl Announced {
    pub fn load(dir: &Path, host: &str) -> Announced {
        let path = dir.join("jobs").join(format!("{host}.json"));
        let read = fs::read(&path).ok();
        let file = read.and_then(|b| serde_json::from_slice::<AnnouncedFile>(&b).ok());
        let file = file.filter(|f| f.version == 1);
        if file.is_none() && path.exists() {
            out::err(&format!(
                "{}: unreadable; the next report is a baseline",
                path.display()
            ));
        }
        Announced {
            path,
            baseline: file.is_none(),
            labels: file.map_or_default(|f| f.announced),
        }
    }

    /// Records `unreviewed` and returns the labels to announce: the new ones, none at the baseline.
    pub fn admit(&mut self, unreviewed: impl Iterator<Item = String>) -> Vec<String> {
        let fresh: Vec<_> = unreviewed.filter(|l| !self.labels.contains(l)).collect();
        if fresh.is_empty() && !self.baseline {
            return fresh;
        }
        self.labels.extend(fresh.iter().cloned());
        let body = serde_json::json!({ "version": 1, "announced": &self.labels }).to_string();
        let tmp = self.path.with_extension("json.tmp");
        let written = self
            .path
            .parent()
            .map(fs::create_dir_all)
            .transpose()
            .and_then(|_| {
                fs::write(&tmp, body)?;
                fs::rename(&tmp, &self.path)
            });
        if let Err(e) = written {
            out::err(&format!("{}: {e}", self.path.display()));
        }
        if std::mem::take(&mut self.baseline) {
            Vec::new()
        } else {
            fresh
        }
    }
}
