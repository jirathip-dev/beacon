use beacon::config::MountPath;
use beacon::metrics::{CAPACITY, DiskUsage, Gauge, Ring, Sample};
use serde::Deserialize;
use std::collections::BTreeMap;

const GOLDEN: &str = include_str!("../../docs/metrics.example.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    version: u8,
    host: String,
    every_s: u64,
    t: Vec<u64>,
    load1: Vec<Option<f64>>,
    load5: Vec<Option<f64>>,
    load15: Vec<Option<f64>>,
    cpu_pct: Vec<Option<f64>>,
    mem: GaugeBody,
    swap: GaugeBody,
    disks: BTreeMap<String, DiskBody>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GaugeBody {
    total_bytes: Option<u64>,
    used_bytes: Vec<Option<u64>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DiskBody {
    used_bytes: Vec<Option<u64>>,
    free_bytes: Vec<Option<u64>>,
}

fn mount(p: &str) -> MountPath {
    MountPath::try_from(p.to_string()).unwrap()
}

fn sample(t: u64, cpu: Option<f32>, disks: &[(&str, u64, u64)]) -> Sample {
    Sample {
        t,
        load: Some([1.523, 1.8, 2.0149]),
        cpu_pct: cpu,
        mem: Some(Gauge {
            total: 34_359_738_368,
            used: 16_622_419_968 + t % 7,
        }),
        swap: Some(Gauge {
            total: 2_147_483_648,
            used: 718_404_485,
        }),
        disks: disks
            .iter()
            .map(|&(m, used, free)| (mount(m), DiskUsage { used, free }))
            .collect(),
    }
}

fn three() -> Ring {
    let mut ring = Ring::new();
    ring.push(sample(
        1_794_787_200,
        None,
        &[
            ("/", 358_843_744_256, 135_488_622_592),
            ("/Volumes/backup", 1000, 2000),
        ],
    ));
    ring.push(Sample {
        load: None,
        mem: None,
        ..sample(
            1_794_787_260,
            Some(12.54),
            &[("/", 358_843_801_600, 135_488_565_248)],
        )
    });
    ring.push(sample(
        1_794_787_320,
        Some(9.8),
        &[
            ("/", 358_843_867_136, 135_488_499_712),
            ("/Volumes/backup", 1500, 1500),
        ],
    ));
    ring
}

#[test]
fn render_gives_the_golden_bytes_in_the_schemas_shape() {
    let got = three().render("macmini", &[mount("/"), mount("/Volumes/backup")]);
    assert_eq!(std::str::from_utf8(&got.body).unwrap(), GOLDEN.trim_end());
    let body: Body = serde_json::from_slice(&got.body).unwrap();
    assert_eq!(
        (body.version, body.host.as_str(), body.every_s),
        (1, "macmini", 60)
    );
    let n = body.t.len();
    assert_eq!(n, 3);
    for col in [&body.load1, &body.load5, &body.load15, &body.cpu_pct] {
        assert_eq!(col.len(), n);
    }
    for col in [&body.mem.used_bytes, &body.swap.used_bytes] {
        assert_eq!(col.len(), n);
    }
    for d in body.disks.values() {
        assert_eq!((d.used_bytes.len(), d.free_bytes.len()), (n, n));
    }
    assert_eq!(body.cpu_pct, [None, Some(12.5), Some(9.8)]);
    assert_eq!(body.load1, [Some(1.52), None, Some(1.52)]);
    assert_eq!(body.load15[0], Some(2.01));
    assert_eq!(body.mem.total_bytes, Some(34_359_738_368));
    assert_eq!(
        body.disks["/Volumes/backup"].free_bytes,
        [Some(2000), None, Some(1500)]
    );
}

#[test]
fn render_keys_disks_by_the_configured_mounts_only() {
    let body: Body = serde_json::from_slice(&three().render("m", &[mount("/new")]).body).unwrap();
    assert_eq!(body.disks.keys().collect::<Vec<_>>(), ["/new"]);
    assert_eq!(body.disks["/new"].used_bytes, [None, None, None]);
}

#[test]
fn empty_ring_renders_empty_columns_and_null_totals() {
    let body: Body = serde_json::from_slice(&Ring::new().render("m", &[mount("/")]).body).unwrap();
    assert!(body.t.is_empty() && body.disks["/"].used_bytes.is_empty());
    assert_eq!((body.mem.total_bytes, body.swap.total_bytes), (None, None));
}

#[test]
fn ring_holds_at_most_1440_and_drops_the_oldest() {
    let mut ring = Ring::new();
    for t in 1..=1500 {
        ring.push(sample(t, None, &[]));
    }
    assert_eq!((ring.len(), CAPACITY), (1440, 1440));
    let body: Body = serde_json::from_slice(&ring.render("m", &[]).body).unwrap();
    assert_eq!((body.t.len(), body.t[0], body.t[1439]), (1440, 61, 1500));
}

#[test]
fn etag_holds_for_the_same_samples_and_changes_with_a_new_one() {
    let disks = [mount("/")];
    let mut ring = three();
    let (a, b) = (ring.render("m", &disks), ring.render("m", &disks));
    assert_eq!(a.etag, b.etag);
    ring.push(sample(1_794_787_380, None, &[]));
    assert_ne!(ring.render("m", &disks).etag, a.etag);
}
