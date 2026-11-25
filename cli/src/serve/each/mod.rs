mod checks;
mod hosts;
mod probes;

use super::hub::{Hub, Loaded};
use std::thread::Scope;

/// Starts the parts that run again for each valid config.
pub fn start<'s>(s: &'s Scope<'s, '_>, hub: &'s Hub, config: &Loaded) {
    checks::start(s, hub, config);
    hosts::start(s, hub, config);
    probes::start(s, hub, config);
}
