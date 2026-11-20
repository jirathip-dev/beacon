import BeaconKit
import Foundation
import Testing

let docs = URL(fileURLWithPath: #filePath).appending(path: "../../../../../docs").standardized
let historyGolden = docs.appending(path: "history.example.json")
let timesGolden = docs.appending(path: "times.example.json")

@Test func decodesGoldenHistoryAndTimes() throws {
    let h = try HistoryBody.decode(Data(contentsOf: historyGolden))
    #expect(h.host == "macmini" && h.gaps == [[1_794_700_000, 1_794_703_600]])
    let backup = try #require(h.checks["backup"]?.down.first)
    #expect(backup.down_at == 1_794_787_200 && backup.up_at == 1_794_790_800 && backup.message == "exit 1")
    #expect(h.checks["disk"]?.down.isEmpty == true)
    let blog = try #require(h.monitors.first)
    #expect(blog.id == "db178b9377caae1a" && blog.group == "Web" && blog.name == "Blog" && blog.track.since == 1_794_000_000)
    #expect(blog.track.down.first?.up_at == nil && blog.track.down.first?.message == "cert expires in 13d")
    let t = try TimesBody.decode(Data(contentsOf: timesGolden))
    #expect(t.id == blog.id && t.every_s == 60)
    #expect(t.recent.t == [1_794_899_880, 1_794_899_940, 1_794_900_000] && t.recent.ms == [212, nil, 198])
    #expect(t.hourly.t == [1_794_898_800] && t.hourly.avg_ms == [205] && t.hourly.max_ms == [212])
    let text = try String(contentsOf: timesGolden, encoding: .utf8)
    #expect(throws: (any Error).self) { try TimesBody.decode(Data(text.replacingOccurrences(of: "[212,null,198]", with: "[212]").utf8)) }
    #expect(throws: (any Error).self) { try HistoryBody.decode(Data(text.utf8)) }
}

@Test func uptimeByHand() throws {
    let h = try HistoryBody.decode(Data(contentsOf: historyGolden))
    let now = 1_794_903_600
    let blog = try #require(h.monitors.first?.track)
    #expect(blog.uptime(last: 86_400, gaps: h.gaps, now: now) == 1 - 3600.0 / 86_400)
    let backup = try #require(h.checks["backup"])
    let young = backup.tally(last: 30 * 86_400, gaps: h.gaps, now: now)
    #expect(young.span == 903_600 && young.watched == 900_000 && young.down == 3600)
    #expect(backup.uptime(last: 30 * 86_400, gaps: h.gaps, now: now) == 1 - 3600.0 / 900_000)
    let body = #"{"version":1,"host":"m","gaps":[[1000,1500]],"checks":{"a":{"since":0,"down":[{"down_at":500,"up_at":2000,"message":"x"}]},"b":{"since":4000,"down":[]}},"monitors":[]}"#
    let g = try HistoryBody.decode(Data(body.utf8))
    #expect(g.checks["a"]?.uptime(last: 4000, gaps: g.gaps, now: 4000) == 1 - 1000.0 / 3500)
    #expect(g.checks["b"]?.uptime(last: 4000, gaps: g.gaps, now: 4000) == nil)
}

@Test func historyURLsAndFetch() async throws {
    let json = #"{"version":1,"prober":"mini","monitors":[{"type":"http","name":"Blog","group":"Web","url":"https://a/","cert_warn_days":30}],"hosts":\#(hosts)}"#
    let c = try JSONDecoder().decode(MonitorsConfig.self, from: Data(json.utf8))
    #expect(c.prober == "mini")
    guard case .http(let blog) = c.monitors.first, blog.cert_warn_days == 30 else { Issue.record("cert_warn_days"); return }
    let host = try #require(c.hosts?["mini"])
    #expect(host.historyURL.absoluteString == "https://mini.ts.net/history")
    #expect(host.historyURL(id: "db178b9377caae1a").absoluteString == "https://mini.ts.net/history/db178b9377caae1a")
    let body = try String(contentsOf: historyGolden, encoding: .utf8)
    let t = MockTransport(replies: [host.historyURL.absoluteString: .success((200, body, ["ETag": "\"h\""]))])
    let reply = await HostMetrics(t).fetch(host.historyURL, etag: nil, timeout: 4, decode: HistoryBody.decode)
    guard case .fresh(let h, "\"h\"") = reply, let m = c.monitors.first else { Issue.record("\(reply)"); return }
    #expect(h.track(of: m)?.track.since == 1_794_000_000 && h.track(of: m)?.id == "db178b9377caae1a")
}

@Test func certificateExpiringWithinItsWindowIsDown() async throws {
    let c = try config("""
        {"type":"http","name":"soon","group":"g","url":"https://a/"},
        {"type":"http","name":"five","group":"g","url":"https://a/","cert_warn_days":5},
        {"type":"http","name":"off","group":"g","url":"https://a/","cert_warn_days":0},
        {"type":"http","name":"far","group":"g","url":"https://b/"}
        """)
    let t = MockTransport(replies: ["https://a/": .success((200, "", [:])), "https://b/": .success((200, "", [:]))])
    t.notAfter = ["https://a/": Date(timeIntervalSince1970: 100 + 10 * 86_400 + 3600),
                  "https://b/": Date(timeIntervalSince1970: 100 + 14 * 86_400)]
    let rows = await run(c, t)
    #expect(rows.map(\.state) == [.down, .up, .up, .up])
    #expect(rows[0].detail == "cert expires in 10d")
}
