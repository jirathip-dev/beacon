use super::super::hub::Hub;
use crate::notify;
use std::thread::Scope;

/// With `--notify`: the delivery thread and the alive thread.
pub fn start<'s>(s: &'s Scope<'s, '_>, hub: &'s Hub) {
    let Some(n) = &hub.notifier else { return };
    s.spawn(|| hub.deliver());
    s.spawn(|| notify::alive(&n.file, &hub.name, || hub.hosts()));
}
