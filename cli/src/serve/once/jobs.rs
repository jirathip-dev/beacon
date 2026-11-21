use super::super::hub::Hub;
use crate::jobs::{self, Manager};
use crate::notify::Event;
use crate::state::Announced;
use std::thread::Scope;

/// Job discovery every 60 s: the rows to the hub, a new unreviewed job announced once.
pub fn start<'s>(s: &'s Scope<'s, '_>, hub: &'s Hub) {
    s.spawn(|| {
        let notifying = hub.notifier.as_ref().and(hub.state.as_deref());
        let mut announced = notifying.map(|d| Announced::load(d, &hub.name));
        jobs::watch(
            &Manager::here(),
            || hub.config().host.jobs.clone(),
            |rows| {
                if let Some(a) = &mut announced {
                    let fresh = a.admit(
                        rows.iter()
                            .filter(|r| r.group.is_none())
                            .map(|r| r.label.clone()),
                    );
                    for r in rows.iter().filter(|r| fresh.contains(&r.label)) {
                        hub.send(Event::job(&hub.name, &r.id, r.status, &r.label));
                    }
                }
                hub.report_jobs(&rows);
            },
        );
    });
}
