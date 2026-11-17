import BeaconKit
import Foundation
import Testing

/// The golden body #13's `cli/tests/metrics.rs` pins to `Ring::render`'s bytes.
let golden = URL(fileURLWithPath: #filePath).appending(path: "../../../../../docs/metrics.example.json").standardized
let metricsURL = "https://mini.ts.net/metrics"

@Test func decodesGoldenMetrics() throws {
    let body = try MetricsBody.decode(Data(contentsOf: golden))
    #expect(body.host == "macmini" && body.every_s == 60)
    #expect(body.t == [1_794_787_200, 1_794_787_260, 1_794_787_320])
    #expect(body.cpu_pct == [nil, 12.5, 9.8])
    #expect(body.mem.total_bytes == 34_359_738_368)
    #expect(body.mounts(order: ["/Volumes/backup", "/gone"]) == ["/Volumes/backup", "/"])
    let series = body.series(order: nil)
    #expect(series.map(\.id) == ["load", "cpu", "mem", "disk /", "disk /Volumes/backup"])
    #expect(series[4].values == [100.0 / 3, nil, 50])
    #expect(series[4].now == 50)
    #expect(series[0].top(series[0].values) == 2)
}

@Test func metricsIgnoresUnknownFieldsRejectsBadShapes() throws {
    let text = try String(contentsOf: golden, encoding: .utf8)
    let extra = text.replacingOccurrences(of: #""every_s":60,"#, with: #""every_s":60,"gpu":{"x":[1]},"#)
        .replacingOccurrences(of: #""total_bytes":2147483648,"#, with: #""total_bytes":2147483648,"z":1,"#)
    #expect(try MetricsBody.decode(Data(extra.utf8)).swap.total_bytes == 2_147_483_648)
    #expect(throws: (any Error).self) {
        try MetricsBody.decode(Data(text.replacingOccurrences(of: #""version":1"#, with: #""version":2"#).utf8))
    }
    #expect(throws: (any Error).self) {
        try MetricsBody.decode(Data(text.replacingOccurrences(of: "[null,12.5,9.8]", with: "[12.5,9.8]").utf8))
    }
}

@Test func windowClipsByTimeBucketsAndKeepsRawStats() {
    let t = Array(stride(from: 0, to: 1500 * 60, by: 60))
    var ys: [Double?] = t.indices.map { Double($0 % 8) }
    ys[1499] = nil
    let hour = Window(t, ys, range: .hour)
    #expect(hour.t.count == 60 && hour.points.count == 60 && hour.missing == 1)
    let day = Window(t, ys, range: .day)
    #expect(day.t.first == 60 * 60 && day.t.count == 1440)
    #expect(day.points.count == 360)
    let firstMean: Double = (4 + 5 + 6 + 7) / 4
    let lastMean: Double = (0 + 1 + 2) / 3
    #expect(day.points[0].t == 63 * 60 && day.points[0].y == firstMean)
    #expect(day.points.last?.y == lastMean)
    #expect(day.max == 7 && day.min == 0)
    let gap = Window([0, 60, 120, 180], [nil, nil, nil, nil], range: .day)
    #expect(gap.points.map(\.y) == [nil] && gap.avg == nil)
}

func metrics(_ reply: Result<MockTransport.Reply, URLError>, etag: String? = nil) async -> (MetricsReply, MockTransport) {
    let transport = MockTransport(replies: [metricsURL: reply])
    return (await HostMetrics(transport).fetch(URL(string: metricsURL)!, etag: etag, timeout: 4), transport)
}

@Test func fetchesMetricsWithETag() async throws {
    let body = try String(contentsOf: golden, encoding: .utf8)
    let (fresh, first) = await metrics(.success((200, body, ["ETag": "\"e1\""])))
    guard case .fresh(let b, let etag) = fresh else { Issue.record("\(fresh)"); return }
    #expect(b.t.count == 3 && etag == "\"e1\"")
    #expect(first.requests[0].0.value(forHTTPHeaderField: "If-None-Match") == nil)
    let (same, second) = await metrics(.success((304, "", [:])), etag: "\"e1\"")
    guard case .unchanged = same else { Issue.record("\(same)"); return }
    #expect(second.requests[0].0.value(forHTTPHeaderField: "If-None-Match") == "\"e1\"")
    #expect(second.requests[0].1 == 4)
    guard case .notServed = await metrics(.success((404, "", [:]))).0 else { Issue.record("404"); return }
    guard case .unreachable("HTTP 500") = await metrics(.success((500, "", [:]))).0 else { Issue.record("500"); return }
    guard case .unreachable("timeout after 4s") = await metrics(.failure(URLError(.timedOut))).0 else {
        Issue.record("timeout"); return
    }
}

@Test func metricsURLReplacesTheLastSegment() throws {
    let c = try config("", hosts: #"{"m":{"url":"https://mini.ts.net/beacon/status","disks":["/"],"checks":[]}}"#)
    #expect(c.hosts?["m"]?.metricsURL.absoluteString == "https://mini.ts.net/beacon/metrics")
    #expect(c.hosts?["m"]?.disks == ["/"])
}
