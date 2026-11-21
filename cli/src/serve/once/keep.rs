use super::super::hub::Hub;
use crate::budget::over_budget;
use crate::clock::every;
use crate::out;
use crate::probe;
use crate::state;
use std::path::Path;
use std::thread::{self, Scope};
use std::time::{Duration, Instant};

const PULL_EVERY_S: u64 = 300;
const TIDY_EVERY_S: u64 = 60;

/// The 1 s loop: the history's tick and save, the state's tidy, `git pull` and the config re-read.
pub fn start<'s>(s: &'s Scope<'s, '_>, hub: &'s Hub) {
    s.spawn(move || {
        let path = hub.path.as_str();
        let dir = Path::new(path).parent().unwrap_or(Path::new(""));
        let git = dir.join(".git").exists();
        let mut seen = Ok(hub.config().raw.clone());
        let (mut pulled, mut tidied) = (None::<Instant>, None::<Instant>);
        let (mut pull_down, mut write_down, mut written) = (false, false, hub.saved.clone());
        loop {
            if let Some(state) = &hub.state
                && tidied.is_none_or(|t| t.elapsed() >= every(TIDY_EVERY_S))
            {
                tidied = Some(Instant::now());
                let r = hub.tick();
                let kept = state::keep_events(state, &r.body, r.etag != written);
                if let Err(e) = &kept
                    && !write_down
                {
                    out::err(&format!("history/events.json: {e}"));
                }
                write_down = kept.is_err();
                written = if write_down { String::new() } else { r.etag };
                state::tidy(state);
            }
            if git && pulled.is_none_or(|p| p.elapsed() >= every(PULL_EVERY_S)) {
                pulled = Some(Instant::now());
                let args = ["-C", &dir.to_string_lossy(), "pull", "--ff-only", "-q"];
                let failed = probe::output("git", &args, Duration::from_secs(60)).is_none();
                if failed && (!pull_down || over_budget("repeat")) {
                    out::err(&format!("git pull --ff-only in {}: failed", dir.display()));
                }
                pull_down = failed;
            }
            let read = std::fs::read(path).map_err(|e| e.to_string());
            if read != seen {
                seen.clone_from(&read);
                match read.and_then(|raw| hub.load(s, raw)) {
                    Ok(n) => out::err(&format!("reload {path}: {n} checks")),
                    Err(e) => out::err(&format!("{path}: {e}; serving the last valid config")),
                }
            }
            thread::sleep(every(1));
        }
    });
}
