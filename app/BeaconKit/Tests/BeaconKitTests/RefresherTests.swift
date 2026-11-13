import BeaconKit
import Foundation
import Testing

/// Answers GETs from a table keyed by URL and records each request; a missing URL throws its error.
final class MockTransport: Transport, @unchecked Sendable {
    typealias Reply = (status: Int, body: String, headers: [String: String])
    private let lock = NSLock()
    private var _requests: [(URLRequest, Int)] = []
    var replies: [String: Result<Reply, URLError>]
    /// A host:port that is absent connects.
    var refused: [String: URLError]

    init(replies: [String: Result<Reply, URLError>] = [:], refused: [String: URLError] = [:]) {
        (self.replies, self.refused) = (replies, refused)
    }

    var requests: [(URLRequest, Int)] { lock.withLock { _requests } }

    func get(_ request: URLRequest, timeout: Int) async throws -> (Data, HTTPURLResponse) {
        let url = request.url!.absoluteString
        let reply = lock.withLock {
            _requests.append((request, timeout))
            return replies[url] ?? .failure(URLError(.cannotFindHost))
        }
        let r = try reply.get()
        return (Data(r.body.utf8), HTTPURLResponse(url: request.url!, statusCode: r.status, httpVersion: nil, headerFields: r.headers)!)
    }

    func connect(host: String, port: Int, timeout: Int) async throws {
        lock.withLock { _requests.append((URLRequest(url: URL(string: "tcp://\(host):\(port)")!), timeout)) }
        if let error = refused["\(host):\(port)"] { throw error }
    }
}

func config(_ monitors: String, hosts: String = "{}") throws -> MonitorsConfig {
    try JSONDecoder().decode(MonitorsConfig.self, from: Data(#"{"version":1,"monitors":[\#(monitors)],"hosts":\#(hosts)}"#.utf8))
}

let hostURL = "https://mini.ts.net/status"
let hosts = #"{"mini":{"url":"\#(hostURL)","timeout_s":4,"checks":[]}}"#
let heartbeats = #"{"type":"heartbeat","name":"Backup","group":"mini","host":"mini","id":"backup"},{"type":"heartbeat","name":"Disk","group":"mini","host":"mini","id":"disk"}"#
let serveBody = #"{"version":1,"host":"mini","future":1,"checks":{"backup":{"status":"down","message":"stale","since":"2026-11-12T03:00:04Z","x":2}}}"#

func run(_ config: MonitorsConfig, _ transport: MockTransport, cache: HostCache = HostCache(),
         previous: [MonitorResult] = [], online: Bool = true, cap: Int? = nil) async -> [MonitorResult] {
    await Refresher(transport: transport, hosts: cache)
        .run(config, previous: previous, online: online, timeoutCap: cap, now: Date(timeIntervalSince1970: 100))
}

@Test func decodesServeResponseIgnoringUnknownFields() throws {
    let body = try StatusBody.decode(Data(serveBody.utf8))
    #expect(body.host == "mini")
    #expect(body.checks["backup"]?.status == .down)
    #expect(body.checks["backup"]?.since == Date(timeIntervalSince1970: 1_794_452_404))
    #expect(throws: (any Error).self) { try StatusBody.decode(Data(serveBody.replacingOccurrences(of: #""version":1"#, with: #""version":2"#).utf8)) }
}

@Test func httpUpDownAndTimeout() async throws {
    let c = try config("""
        {"type":"http","name":"ok","group":"g","url":"https://a/","keyword":"hi"},
        {"type":"http","name":"503","group":"g","url":"https://b/"},
        {"type":"http","name":"nokw","group":"g","url":"https://a/","keyword":"bye"},
        {"type":"http","name":"expect","group":"g","url":"https://c/","expect_status":401},
        {"type":"http","name":"slow","group":"g","url":"https://d/","timeout_s":7}
        """)
    let t = MockTransport(replies: [
        "https://a/": .success((200, "say hi", [:])), "https://b/": .success((503, "", [:])),
        "https://c/": .success((401, "", [:])), "https://d/": .failure(URLError(.timedOut)),
    ])
    let rows = await run(c, t)
    #expect(rows.map(\.state) == [.up, .down, .down, .up, .down])
    #expect(rows.map(\.detail) == ["", "HTTP 503", "keyword missing: bye", "", "timeout after 7s"])
    #expect(rows[0].since == Date(timeIntervalSince1970: 100))
}

@Test func tcpUpDownAndTimeout() async throws {
    let c = try config("""
        {"type":"tcp","name":"a","group":"g","host":"a","port":22},
        {"type":"tcp","name":"b","group":"g","host":"b","port":22},
        {"type":"tcp","name":"c","group":"g","host":"c","port":22,"timeout_s":3}
        """)
    let t = MockTransport(refused: ["b:22": URLError(.cannotConnectToHost), "c:22": URLError(.timedOut)])
    let rows = await run(c, t)
    #expect(rows.map(\.state) == [.up, .down, .down])
    #expect(rows[2].detail == "timeout after 3s")
}

@Test func backgroundCapsEveryTimeoutAt10s() async throws {
    let c = try config(#"{"type":"http","name":"a","group":"g","url":"https://a/","timeout_s":30},"#
        + #"{"type":"tcp","name":"b","group":"g","host":"b","port":1,"timeout_s":20}"#)
    let t = MockTransport(replies: ["https://a/": .failure(URLError(.timedOut))])
    let rows = await run(c, t, cap: 10)
    #expect(t.requests.map(\.1) == [10, 10])
    #expect(rows[0].detail == "timeout after 10s")
}

@Test func heartbeatPresentAndMissingId() async throws {
    let t = MockTransport(replies: [hostURL: .success((200, serveBody, [:]))])
    let rows = await run(try config(heartbeats, hosts: hosts), t)
    #expect(rows[0] == MonitorResult(name: "Backup", group: "mini", state: .down, detail: "stale",
                                     since: Date(timeIntervalSince1970: 1_794_452_404)))
    #expect(rows[1].state == .down && rows[1].detail == "no result yet")
    #expect(t.requests.count == 1, "one GET per host, not per heartbeat")
    #expect(t.requests[0].1 == 4)
}

@Test(arguments: [
    (Result<MockTransport.Reply, URLError>.failure(URLError(.timedOut)), "host unreachable: timeout after 4s"),
    (.success((502, "", [:])), "host unreachable: HTTP 502"),
    (.success((200, "{}", [:])), "host unreachable: version: missing"),
])
func heartbeatHostUnreachable(reply: Result<MockTransport.Reply, URLError>, detail: String) async throws {
    let rows = await run(try config(heartbeats, hosts: hosts), MockTransport(replies: [hostURL: reply]))
    #expect(rows.map(\.state) == [.down, .down])
    #expect(rows.map(\.detail) == [detail, detail])
}

@Test func hostFetchSendsETagAndUses304Body() async throws {
    let t = MockTransport(replies: [hostURL: .success((200, serveBody, ["ETag": "\"v1\""]))])
    let cache = HostCache()
    let c = try config(heartbeats, hosts: hosts)
    _ = await run(c, t, cache: cache)
    t.replies[hostURL] = .success((304, "", [:]))
    let rows = await run(c, t, cache: cache)
    #expect(t.requests[1].0.value(forHTTPHeaderField: "If-None-Match") == "\"v1\"")
    #expect(rows[0].detail == "stale")
}

@Test func offlineMakesEveryRowUnknownWithoutProbing() async throws {
    let t = MockTransport()
    let rows = await run(try config(heartbeats + #",{"type":"tcp","name":"a","group":"g","host":"a","port":1}"#, hosts: hosts), t, online: false)
    #expect(rows.map(\.state) == [.unknown, .unknown, .unknown])
    #expect(t.requests.isEmpty)
}

@Test func sinceCarriesOverWhileStateHolds() async throws {
    let c = try config(#"{"type":"http","name":"a","group":"g","url":"https://a/"}"#)
    let t = MockTransport(replies: ["https://a/": .success((200, "", [:]))])
    let earlier = Date(timeIntervalSince1970: 5)
    let up = await run(c, t, previous: [MonitorResult(name: "a", group: "g", state: .up, detail: "", since: earlier)])
    #expect(up[0].since == earlier)
    let flipped = await run(c, t, previous: [MonitorResult(name: "a", group: "g", state: .down, detail: "", since: earlier)])
    #expect(flipped[0].since == Date(timeIntervalSince1970: 100))
}

@Test func alertsOnUpToDownAndRecoveryOnly() {
    func r(_ name: String, _ state: ResultState) -> MonitorResult {
        MonitorResult(name: name, group: "g", state: state, detail: "HTTP 503", since: nil)
    }
    let old = [r("a", .up), r("b", .down), r("c", .up), r("d", .unknown), r("e", .down)]
    let new = [r("a", .down), r("b", .up), r("c", .up), r("d", .down), r("e", .down), r("f", .down)]
    #expect(alerts(from: old, to: new).map { [$0.title, $0.body] } == [["a down", "HTTP 503"], ["b recovered", "g"]])
}
