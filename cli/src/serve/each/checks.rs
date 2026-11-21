use super::super::hub::{Hub, Loaded, Of};
use crate::clock::every;
use crate::probe;
use std::thread::{self, Scope};
use std::time::Duration;

/// One thread per check of `config`, each ending with its generation.
pub fn start<'s>(s: &'s Scope<'s, '_>, hub: &'s Hub, config: &Loaded) {
    let generation = config.generation;
    for c in config.host.checks.clone() {
        s.spawn(move || {
            loop {
                let timeout = Duration::from_secs(c.timeout_s.into());
                let (st, message) = hub.hold(|| probe::run(&c.command, timeout));
                if !hub.report(generation, Of::Check(&c.id), st, message) {
                    return;
                }
                thread::sleep(every(c.every_s.into()));
            }
        });
    }
}
