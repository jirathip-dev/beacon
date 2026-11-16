# Data shapes

The wire shapes are the two JSON schemas beside this file, and they are the contract. Each side
writes its own types from them by hand (ADR D7). These are the cross-field rules the schemas cannot express, and `beacon check` enforces them
(exit 2):

- A heartbeat monitor's `host` is a key of `hosts`, and its `id` is a check id on that host.
- Check ids are unique within their host.
- A check's `timeout_s` is less than its `every_s`.

The schemas say `additionalProperties: false`. That describes what is valid to write and what the
CLI enforces. The app reads leniently: it ignores unknown fields (D1) and does not validate
against the schema.

## Rust (`cli/src`)

```rust
// config.rs: pure parse + cross-field rules
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
pub struct MonitorsConfig { pub version: Version1, pub monitors: Vec<Monitor>,
                            #[serde(default)] pub hosts: BTreeMap<Id, Host> }
#[derive(Deserialize)] #[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Monitor { Http(HttpMonitor), Tcp(TcpMonitor), Heartbeat(HeartbeatMonitor) }
pub struct HeartbeatMonitor { pub name: String, pub group: String, pub host: Id, pub id: Id }
pub struct Host { pub url: Url, #[serde(default = "five")] pub timeout_s: u8, pub checks: Vec<HostCheck> }
#[derive(Deserialize, PartialEq, Clone)]          // PartialEq: reload restarts only changed checks
pub struct HostCheck { pub id: Id, pub command: String,
                       #[serde(default = "sixty")] pub every_s: u32, #[serde(default = "thirty")] pub timeout_s: u16 }
pub struct Id(String);                           // ^[a-z0-9][a-z0-9-]{0,63}$, checked on parse
pub struct Version1;                             // deserializes only from 1
```

`Http` and `Tcp` are parsed and validated by the CLI but never probed by it.

```rust
// status.rs: serve's response and its cache; pure
#[derive(Serialize, Deserialize)] #[serde(deny_unknown_fields)]
pub struct StatusBody { pub version: Version1, pub host: String, pub checks: BTreeMap<Id, CheckResult> }
#[derive(Serialize, Deserialize, PartialEq, Clone)]
pub struct CheckResult { pub status: Status, pub message: String, pub since: Timestamp }  // RFC 3339, UTC, seconds
#[derive(Serialize, Deserialize, PartialEq, Clone, Copy)] #[serde(rename_all = "lowercase")]
pub enum Status { Up, Down }

pub struct Rendered { pub body: Arc<[u8]>, pub etag: String }  // what a request reads
impl StatusBody {
    /// Records one run. Returns true when status or message changed (then re-render).
    pub fn record(&mut self, id: &Id, status: Status, message: String, now: Timestamp) -> bool;
    pub fn render(&self) -> Rendered;           // serde_json bytes + hash
}
```

`since` changes only when `status` changes. A changed `message` with the same status keeps `since`.
There is no local state file: serve's memory is the whole state, and a restart begins empty (ADR D3).

## Swift (`app/BeaconKit`)

```swift
struct MonitorsConfig: Decodable { let version: Int; let monitors: [Monitor]; let hosts: [String: Host]? }
enum Monitor: Decodable { case http(HTTPMonitor), tcp(TCPMonitor), heartbeat(HeartbeatMonitor) }  // keyed on "type"
struct HeartbeatMonitor: Decodable { let name: String; let group: String; let host: String; let id: String }
struct Host: Decodable { let url: URL; let timeout_s: Int? }      // the app never decodes `checks`

struct StatusBody: Decodable { let version: Int; let host: String; let checks: [String: CheckResult] }
struct CheckResult: Decodable { let status: Status; let message: String; let since: Date }
enum Status: String, Decodable { case up, down }

enum ConfigSource: Codable { case url(URL, headerName: String?)  // the header value is in the Keychain
                             case local }                         // App Group file monitors.json

struct MonitorResult: Codable { let name: String; let group: String; let state: ResultState; let detail: String; let checkedAt: Date }
enum ResultState: String, Codable { case up, down, unknown }      // unknown: the phone has no network (D5)
struct StatusSnapshot: Codable { let rows: [MonitorResult]; let at: Date; let configAt: Date; var worst: ResultState }  // App Group file
```

Swift decoding (D1): a `version != 1` or an unknown monitor `type` throws. Unknown keys are
ignored, which is what `Decodable` does by default, so there is no `allKeys` check and no `CodingKeys` beyond the
`type` discriminator. `Host.checks` is never decoded, because the app does not run host checks. The Rust side
keeps `deny_unknown_fields`.
