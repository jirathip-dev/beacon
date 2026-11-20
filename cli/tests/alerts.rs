use beacon::config::{self, MountPath};
use beacon::metrics::{DiskUsage, Gauge, Sample};
use beacon::notify::{Alerts, Event, Timing, Tracker};
use beacon::rules::{self, Streak};
use beacon::status::Status;
use serde_json::{Value, json};
use std::time::Duration;

const S: Duration = Duration::from_secs(1);

fn timing(remind_every_h: u16, remind_max: u8) -> Timing {
    let policy = config::parse(
        &json!({ "version": 1, "monitors": [], "notify": { "remind_every_h": remind_every_h, "remind_max": remind_max } })
            .to_string(),
    )
    .unwrap()
    .notify;
    Timing {
        group: 30 * S,
        hour: 3600 * S,
        policy,
    }
}

fn down(t: &mut Tracker, check: &str) -> Event {
    t.check(check, Status::Up, "ok");
    t.check(check, Status::Down, "bad");
    t.check(check, Status::Down, "bad").unwrap()
}

fn host_down(t: &mut Tracker) -> Event {
    t.polled(&Ok(()));
    (0..3)
        .find_map(|_| t.polled(&Err("refused".into())))
        .unwrap()
}

fn sent(alerts: &mut Alerts, from: Duration, to: Duration, t: &Timing) -> Vec<(Duration, Value)> {
    let mut out = Vec::new();
    let mut now = from;
    while now <= to {
        if let Some(b) = alerts.due(now, t) {
            out.push((now, serde_json::to_value(&b).unwrap()));
        }
        now += S;
    }
    out
}

#[test]
fn five_changes_within_the_window_give_one_batch_listing_5_sent_when_it_closes() {
    let (t, mut alerts, mut box_) = (timing(4, 3), Alerts::default(), Tracker::new("box"));
    for (i, id) in ["a", "b", "c", "d", "e"].iter().enumerate() {
        alerts.change(down(&mut box_, id), u32::try_from(i).unwrap() * 5 * S);
    }
    assert_eq!(alerts.wait(25 * S, &t), Some(5 * S));
    let got = sent(&mut alerts, Duration::ZERO, 120 * S, &t);
    assert_eq!(got.len(), 1, "{got:?}");
    let (at, b) = &got[0];
    assert_eq!(*at, 30 * S);
    assert_eq!(b["kind"], "batch");
    assert_eq!(b["summary"], "5 checks down");
    let ids: Vec<_> = b["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["check"].clone())
        .collect();
    assert_eq!(ids, ["a", "b", "c", "d", "e"]);
}

#[test]
fn a_single_change_is_a_one_item_batch_named_by_its_host_and_check() {
    let (t, mut alerts, mut box_) = (timing(4, 3), Alerts::default(), Tracker::new("box"));
    alerts.change(down(&mut box_, "disk"), Duration::ZERO);
    let got = sent(&mut alerts, Duration::ZERO, 60 * S, &t);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].1["summary"], "box/disk down");
    assert_eq!(got[0].1["changes"].as_array().unwrap().len(), 1);
}

#[test]
fn a_host_down_plus_its_3_checks_give_one_item_and_its_checks_get_no_reminders() {
    let (t, mut alerts) = (timing(4, 3), Alerts::default());
    let (mut boat, mut other) = (Tracker::new("boat-dev-2"), Tracker::new("mini"));
    for id in ["a", "b", "c"] {
        alerts.change(down(&mut boat, id), Duration::ZERO);
    }
    alerts.change(host_down(&mut boat), 10 * S);
    let got = sent(&mut alerts, Duration::ZERO, 60 * S, &t);
    assert_eq!(got.len(), 1, "{got:?}");
    let changes = got[0].1["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 1, "{changes:?}");
    assert_eq!(
        (&changes[0]["kind"], &changes[0]["host"]),
        (&json!("host"), &json!("boat-dev-2"))
    );
    alerts.change(down(&mut other, "x"), 100 * S);
    let got = sent(&mut alerts, 100 * S, 140 * S, &t);
    assert_eq!(got[0].1["summary"], "mini/x down");
    let got = sent(&mut alerts, 4 * 3600 * S, 4 * 3600 * S + 60 * S, &t);
    let reminded: Vec<_> = got[0].1["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["message"].clone())
        .collect();
    assert_eq!(reminded, ["still down: boat-dev-2 for 4h"]);
}

#[test]
fn reminders_fire_every_remind_every_h_batched_together_and_stop_after_remind_max() {
    let (t, mut alerts, mut box_) = (timing(2, 3), Alerts::default(), Tracker::new("box"));
    alerts.change(down(&mut box_, "a"), Duration::ZERO);
    alerts.change(down(&mut box_, "b"), Duration::ZERO);
    let got = sent(&mut alerts, Duration::ZERO, 12 * 3600 * S, &t);
    let at: Vec<_> = got.iter().map(|(at, _)| at.as_secs()).collect();
    assert_eq!(at, [30, 7230, 14430, 21630]);
    assert_eq!(got[1].1["summary"], "2 still down");
    let msgs: Vec<_> = got[3].1["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["message"].clone())
        .collect();
    assert_eq!(
        msgs,
        ["still down: box/a for 6h", "still down: box/b for 6h"]
    );
    assert_eq!(alerts.wait(12 * 3600 * S, &t), None);
}

#[test]
fn remind_max_0_sends_no_reminder_and_a_recovery_ends_them() {
    let (none, mut alerts, mut box_) = (timing(1, 0), Alerts::default(), Tracker::new("box"));
    alerts.change(down(&mut box_, "a"), Duration::ZERO);
    assert_eq!(
        sent(&mut alerts, Duration::ZERO, 10 * 3600 * S, &none).len(),
        1
    );
    let t = timing(1, 3);
    let mut alerts = Alerts::default();
    alerts.change(down(&mut box_, "b"), Duration::ZERO);
    box_.check("b", Status::Up, "ok");
    let up = box_.check("b", Status::Up, "ok").unwrap();
    alerts.change(up, 10 * S);
    let got = sent(&mut alerts, Duration::ZERO, 10 * 3600 * S, &t);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].1["summary"], "1 check down, 1 check up");
}

#[test]
fn a_rule_above_for_less_than_for_min_stays_up_and_past_it_goes_down_once_then_up_after_for_min_below()
 {
    let host: Value = json!({ "url": "https://x/status", "checks": [], "disks": ["/", "/Volumes/T7"],
        "rules": [{ "id": "full", "metric": "disk", "above": 90, "for_min": 3 }] });
    let config =
        config::parse(&json!({ "version": 1, "monitors": [], "hosts": { "x": host } }).to_string())
            .unwrap();
    let checks = config.hosts.values().next().unwrap().rule_checks().unwrap();
    let ids: Vec<_> = checks.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["full", "full-volumes-t7"]);
    let sample = |pct: u64| Sample {
        disks: [(
            MountPath::try_from("/".to_string()).unwrap(),
            DiskUsage {
                used: pct,
                free: 100 - pct,
            },
        )]
        .into(),
        ..Sample::default()
    };
    let mut streak = Streak::default();
    let mut seen = Vec::new();
    for pct in [
        95, 95, 95, 50, 95, 95, 95, 95, 95, 95, 80, 80, 80, 95, 80, 80, 80, 80,
    ] {
        seen.push(rules::judge(&checks[0], &sample(pct), 1.0, &mut streak).0);
    }
    let flips: Vec<_> = seen
        .windows(2)
        .enumerate()
        .filter(|(_, w)| w[0] != w[1])
        .map(|(i, w)| (i + 1, w[1]))
        .collect();
    assert_eq!(flips, [(7, Status::Down), (17, Status::Up)]);
    let (_, msg) = rules::judge(&checks[0], &sample(93), 1.0, &mut Streak::default());
    assert_eq!(msg, "disk / 93.0% used (down above 90 for 3 min)");
}

#[test]
fn memory_and_load_rules_read_used_percent_and_load1_per_core_and_defaults_apply() {
    let host = json!({ "url": "https://x/status", "checks": [] });
    let config =
        config::parse(&json!({ "version": 1, "monitors": [], "hosts": { "x": host } }).to_string())
            .unwrap();
    let checks = config.hosts.values().next().unwrap().rule_checks().unwrap();
    let ids: Vec<_> = checks
        .iter()
        .map(|c| (c.id.as_str(), c.rule.above, c.rule.for_min))
        .collect();
    assert_eq!(
        ids,
        [
            ("disk-full", 90.0, 10),
            ("memory-full", 90.0, 10),
            ("load-high", 2.0, 15)
        ]
    );
    let s = Sample {
        mem: Some(Gauge {
            total: 200,
            used: 190,
        }),
        load: Some([9.0, 1.0, 1.0]),
        ..Sample::default()
    };
    let mut streak = Streak::default();
    assert_eq!(
        rules::judge(&checks[1], &s, 4.0, &mut streak).1,
        "memory 95.0% used (down above 90 for 10 min)"
    );
    assert_eq!(
        rules::judge(&checks[2], &s, 4.0, &mut streak).1,
        "load 2.2 per core (down above 2 for 15 min)"
    );
    assert_eq!(
        rules::judge(&checks[0], &s, 4.0, &mut streak).1,
        "disk /: no reading"
    );
}
