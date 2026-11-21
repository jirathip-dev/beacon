use super::super::hub::{Hub, Loaded, Of};
use crate::monitor::Seen;
use crate::prober;
use std::thread::Scope;

/// On the prober host: the monitor probes of `config`, their results and `/history/<id>` to the hub.
pub fn start<'s>(s: &'s Scope<'s, '_>, hub: &'s Hub, config: &Loaded) {
    let monitors = config.monitors.clone();
    let record = move |generation, m: &_, seen: &Seen, times| {
        hub.report(
            generation,
            Of::Monitor(m, times),
            seen.status,
            seen.message.clone(),
        )
    };
    prober::run(s, config.generation, monitors, hub.times.as_deref(), record);
}
