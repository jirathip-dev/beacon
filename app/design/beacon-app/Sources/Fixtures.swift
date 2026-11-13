import SwiftUI

// Preview-only fixtures. No BeaconKit, no networking: impl replaces these with
// MonitorResult / StatusSnapshot (docs/architecture/beacon/types.md).

enum Health { case up, down, unknown }

struct Row: Identifiable {
    let id = UUID()
    let name: String
    let health: Health
    let detail: String  // error text; empty when up or unknown
    let since: String  // "since" stamp: HH:mm today, "MMM d" earlier, empty when unknown
}

struct MonitorGroup: Identifiable {
    var id: String { name }
    let name: String
    let rows: [Row]
}

struct ListState {
    let groups: [MonitorGroup]
    let configFrom: String?  // set only when the config fetch failed (ADR D2)
    let offline: Bool  // NWPathMonitor unsatisfied: every row unknown (ADR D5)
}

struct WidgetState {
    let up: Int
    let downNames: [String]
    let checkedAt: String
}

enum Fixture {
    static let groups = [
        MonitorGroup(name: "Web", rows: [
            Row(name: "Blog", health: .up, detail: "", since: "Nov 11"),
            Row(name: "API", health: .down, detail: "HTTP 503", since: "09:28"),
        ]),
        MonitorGroup(name: "Hosts", rows: [
            Row(name: "SSH macmini", health: .up, detail: "", since: "Nov 2"),
        ]),
        MonitorGroup(name: "macmini", rows: [
            Row(name: "Nightly backup", health: .up, detail: "", since: "03:12"),
            Row(name: "Disk space", health: .down, detail: "93% used", since: "08:55"),
        ]),
        MonitorGroup(name: "nas", rows: [
            Row(name: "Snapshots", health: .down, detail: "host unreachable: timed out after 5s", since: "09:30"),
            Row(name: "Scrub", health: .down, detail: "no result yet", since: "09:30"),
        ]),
    ]

    static let list = ListState(groups: groups, configFrom: "08:40", offline: false)

    static let offline = ListState(
        groups: groups.map { g in
            MonitorGroup(name: g.name, rows: g.rows.map { Row(name: $0.name, health: .unknown, detail: "", since: "") })
        },
        configFrom: nil, offline: true)

    static let widgetDown = WidgetState(up: 3, downNames: ["API", "Disk space", "Snapshots", "Scrub"], checkedAt: "09:31")
    static let widgetUp = WidgetState(up: 7, downNames: [], checkedAt: "09:31")

    static let configURL = "https://gist.githubusercontent.com/jk/beacon/raw/monitors.json"
    static let decodeError = "monitors[2]: unknown type \"ping\""
}
