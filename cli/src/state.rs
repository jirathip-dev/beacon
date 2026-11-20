use crate::history::KEEP_S;
use crate::serve::over_budget;
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
    for entry in fs::read_dir(times(dir)).into_iter().flatten().flatten() {
        if entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|t| t < old)
        {
            fs::remove_file(entry.path()).ok();
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
        if !path.ends_with("serve.log") && fs::remove_file(&path).is_ok() {
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
