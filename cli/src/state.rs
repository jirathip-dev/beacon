use crate::serve::over_budget;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
