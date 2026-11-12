# Data shapes

Wire shapes are the two JSON schemas beside this file; they are the contract. Each side
hand-writes its types from them (ADR D7). Cross-file rules the schemas cannot say:

- heartbeat `id` is unique across `monitors`.
- every `hosts.*[].id` names a heartbeat monitor's `id`.
- `max_age_min` should be ≥ 40 (`beacon check` warns, does not fail).

## Rust (`cli/src/config.rs`)

```rust
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
pub struct MonitorsConfig { pub version: Version1, pub monitors: Vec<Monitor>,
                            #[serde(default)] pub hosts: BTreeMap<HostName, Vec<HostCheck>> }
#[derive(Deserialize)] #[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Monitor { Http(HttpMonitor), Tcp(TcpMonitor), Heartbeat(HeartbeatMonitor) }
pub struct HostCheck { pub id: BeatId, pub command: String, #[serde(default)] pub timeout_s: Timeout }
pub struct BeatId(String);                       // validated by the schema's pattern on parse
```

Only what `beacon run` needs gets behaviour; `Http`/`Tcp` are parsed and validated, not checked, by the CLI.

```rust
// cli/src/beat.rs — the wire file
#[derive(Serialize, Deserialize)] #[serde(deny_unknown_fields)]
pub struct Beat { pub id: BeatId, pub status: Status, pub message: String, pub host: String,
                  pub time: DateTime<Utc> }
#[derive(Serialize, Deserialize, PartialEq, Clone, Copy)] #[serde(rename_all = "lowercase")]
pub enum Status { Up, Down }

// cli/src/state.rs — local, $XDG_STATE_HOME/beacon/state.json
#[derive(Serialize, Deserialize, Default)]
pub struct State { pub version: u8, pub beats: BTreeMap<BeatId, Written>, pub monitors_etag: Option<String> }
pub struct Written { pub status: Status, pub written_at: DateTime<Utc>, pub sha: String }
```

## Swift (`app/BeaconKit`)

```swift
struct MonitorsConfig: Decodable { let version: Int; let monitors: [Monitor]; let hosts: [String: [HostCheck]]? }
enum Monitor: Decodable { case http(HTTPMonitor), tcp(TCPMonitor), heartbeat(HeartbeatMonitor) }   // keyed on "type"
struct Beat: Decodable { let id: String; let status: Status; let message: String; let host: String; let time: Date }
enum Status: String, Decodable { case up, down }

struct MonitorResult: Codable { let name: String; let group: String; let state: ResultState; let detail: String; let checkedAt: Date }
enum ResultState: String, Codable { case up, down, unknown }   // unknown: check could not run (no network, no token)
struct StatusSnapshot: Codable { let rows: [MonitorResult]; let at: Date; var worst: ResultState }  // App Group file
```

Swift decoding is strict too: a `version != 1` or an unknown `type` throws; unknown keys are
rejected by decoding each monitor through an explicit `CodingKeys` set and comparing against the container's `allKeys`.
