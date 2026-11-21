use super::super::hub::{Hub, Loaded};
use std::thread::Scope;

/// With `--notify`: one thread per other host of `config` not watched already.
pub fn start<'s>(s: &'s Scope<'s, '_>, hub: &'s Hub, config: &Loaded) {
    let born = config.generation;
    for id in hub.watched(config) {
        s.spawn(move || hub.watch(&id, born));
    }
}
