import BeaconKit
import Foundation
import Testing

private let url = URL(string: "https://mini.example.ts.net/config")!
private let body = #"{"version":1,"monitors":[{"type":"tcp","name":"a","group":"g","host":"a","port":1}]}"#

private func store(_ t: MockTransport) -> ConfigStore {
    ConfigStore(directory: .temporaryDirectory.appending(path: UUID().uuidString), transport: t)
}

private let t0 = Date(timeIntervalSince1970: 0), t1 = Date(timeIntervalSince1970: 60)

@Test func fetchCachesWithETagThen304UsesCache() async throws {
    let t = MockTransport(replies: [url.absoluteString: .success((200, body, ["ETag": "\"e1\""]))])
    let s = store(t)
    let first = try await s.load(url, now: t0)
    #expect(first.config.monitors.count == 1 && first.failure == nil)
    #expect(t.requests[0].0.value(forHTTPHeaderField: "If-None-Match") == nil)
    #expect(t.requests[0].0.allHTTPHeaderFields?.isEmpty ?? true, "no token or other header is sent")

    t.replies[url.absoluteString] = .success((304, "", [:]))
    let second = try await s.load(url, now: t1)
    #expect(t.requests[1].0.value(forHTTPHeaderField: "If-None-Match") == "\"e1\"")
    #expect(second.config.monitors.count == 1 && second.failure == nil && second.at == t1)
}

@Test func failedFetchUsesCacheAndSaysWhy() async throws {
    let t = MockTransport(replies: [url.absoluteString: .success((200, body, [:]))])
    let s = store(t)
    _ = try await s.load(url, now: t0)
    t.replies[url.absoluteString] = .failure(URLError(.cannotFindHost))
    let offTailnet = try await s.load(url, now: t1)
    #expect(offTailnet.failure == "can't reach the Mini: is Tailscale on?" && offTailnet.at == t0)
    t.replies[url.absoluteString] = .success((404, "", [:]))
    #expect(try await s.load(url, now: t1).failure == "HTTP 404")
    t.replies[url.absoluteString] = .success((200, "{\"version\":1}", [:]))
    #expect(try await s.load(url, now: t1).failure == "not a Beacon config: monitors: missing", "a bad body keeps the cache")
}

@Test func failedConnectWithoutCacheOfThatURLSaysWhy() async throws {
    let t = MockTransport(replies: [url.absoluteString: .success((200, body, [:]))])
    let s = store(t)
    _ = try await s.load(url)
    let other = URL(string: "https://other.example/config")!
    t.replies[other.absoluteString] = .failure(URLError(.timedOut))
    await #expect { try await s.load(other) } throws: { describe($0) == "can't reach the Mini: is Tailscale on?" }
    t.replies[other.absoluteString] = .success((404, "", [:]))
    await #expect { try await s.load(other) } throws: { describe($0) == "HTTP 404" }
    t.replies[other.absoluteString] = .success((200, "<html>", [:]))
    await #expect { try await s.load(other) } throws: { describe($0).hasPrefix("not a Beacon config") }
}

@Test func savedSourceKeepsAHeaderlessURLAndDropsHeaderOrLocal() throws {
    #expect(ConfigSource.url(from: ConfigSource.data(url)) == url)
    #expect(ConfigSource.url(from: Data(#"{"url":{"_0":"https:\/\/gist.example\/m.json"}}"#.utf8))?.host() == "gist.example")
    #expect(ConfigSource.url(from: Data(#"{"url":{"_0":"https://gist.example/m.json","headerName":"Authorization"}}"#.utf8)) == nil)
    #expect(ConfigSource.url(from: Data(#"{"local":{}}"#.utf8)) == nil)
}
