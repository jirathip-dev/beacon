mod jobs;
mod keep;
mod sample;
mod send;
mod waste;

use super::hub::Hub;
use std::thread::Scope;

/// Starts the parts that live as long as serve.
pub fn start<'s>(s: &'s Scope<'s, '_>, hub: &'s Hub) {
    jobs::start(s, hub);
    keep::start(s, hub);
    sample::start(s, hub);
    send::start(s, hub);
    waste::start(s, hub);
}
