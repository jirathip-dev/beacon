import BeaconKit
import Foundation
import Testing

private let exampleURL = URL(filePath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent()
    .appending(path: "docs/monitors.example.json")

private func example() throws -> [String: Any] {
    try JSONSerialization.jsonObject(with: Data(contentsOf: exampleURL)) as! [String: Any]
}

private func decode(_ object: Any) throws -> MonitorsConfig {
    try JSONDecoder().decode(MonitorsConfig.self, from: JSONSerialization.data(withJSONObject: object))
}

@Test func exampleDecodesEveryMonitorType() throws {
    let config = try JSONDecoder().decode(MonitorsConfig.self, from: Data(contentsOf: exampleURL))
    let kinds = Set(config.monitors.map { m -> String in
        switch m {
        case .http: "http"
        case .tcp: "tcp"
        case .heartbeat: "heartbeat"
        }
    })
    #expect(kinds == ["http", "tcp", "heartbeat"])
    #expect(config.hosts?["macmini"]?.url.absoluteString == "https://macmini.tail1234.ts.net/status")
}

@Test func ignoresUnknownFields() throws {
    var json = try example()
    json["future"] = true
    var monitors = json["monitors"] as! [[String: Any]]
    monitors[0]["future"] = 1
    json["monitors"] = monitors
    #expect(try decode(json).monitors.count == 5)
}

@Test func rejectsUnknownVersionAndType() throws {
    var json = try example()
    json["version"] = 2
    #expect(throws: DecodingError.self) { try decode(json) }

    json = try example()
    var monitors = json["monitors"] as! [[String: Any]]
    monitors[0]["type"] = "ping"
    json["monitors"] = monitors
    #expect(throws: DecodingError.self) { try decode(json) }
}
