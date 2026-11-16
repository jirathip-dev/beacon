import BeaconKit
import Foundation
import Testing

private let url = URL(string: "https://gist.example/monitors.json")!
private let body = #"{"version":1,"monitors":[{"type":"tcp","name":"a","group":"g","host":"a","port":1}]}"#

private func store(_ t: MockTransport) -> ConfigStore {
    ConfigStore(directory: .temporaryDirectory.appending(path: UUID().uuidString), transport: t, headers: MockHeaders())
}

private let t0 = Date(timeIntervalSince1970: 0), t1 = Date(timeIntervalSince1970: 60)

@Test func fetchCachesWithETagThen304UsesCache() async throws {
    let t = MockTransport(replies: [url.absoluteString: .success((200, body, ["ETag": "\"e1\""]))])
    let s = store(t)
    let source = ConfigSource.url(url, headerName: "Authorization")
    let first = try await s.load(source, header: "Bearer x", now: t0)
    #expect(first.config.monitors.count == 1 && first.failure == nil)
    #expect(t.requests[0].0.value(forHTTPHeaderField: "Authorization") == "Bearer x")
    #expect(t.requests[0].0.value(forHTTPHeaderField: "If-None-Match") == nil)

    t.replies[url.absoluteString] = .success((304, "", [:]))
    let second = try await s.load(source, header: "Bearer x", now: t1)
    #expect(t.requests[1].0.value(forHTTPHeaderField: "If-None-Match") == "\"e1\"")
    #expect(second.config.monitors.count == 1 && second.failure == nil)
}

@Test func failedFetchUsesCacheAndIsStale() async throws {
    let t = MockTransport(replies: [url.absoluteString: .success((200, body, [:]))])
    let s = store(t)
    _ = try await s.load(.url(url, headerName: nil), header: nil, now: t0)
    t.replies[url.absoluteString] = .failure(URLError(.notConnectedToInternet))
    let loaded = try await s.load(.url(url, headerName: nil), header: nil, now: t1)
    #expect(loaded.failure != nil && loaded.at == t0)

    t.replies[url.absoluteString] = .success((200, "{\"version\":1}", [:]))
    #expect(try await s.load(.url(url, headerName: nil), header: nil, now: t1).failure != nil, "a bad body keeps the cache")
}

@Test func failedFetchWithoutCacheOfThatURLThrows() async throws {
    let t = MockTransport(replies: [url.absoluteString: .success((200, body, [:]))])
    let s = store(t)
    _ = try await s.load(.url(url, headerName: nil), header: nil)
    let other = URL(string: "https://other.example/m.json")!
    await #expect(throws: URLError.self) { try await s.load(.url(other, headerName: nil), header: nil) }
}

@Test func importLocalRefusesInvalidFileWithDecodeError() async throws {
    let s = store(MockTransport())
    let bad = body.replacingOccurrences(of: "\"tcp\"", with: "\"ping\"")
    #expect {
        try s.importLocal(Data(bad.utf8))
    } throws: { describe($0) == "monitors[0].type: unknown monitor type ping" }
    _ = try s.importLocal(Data(body.utf8), now: t0)
    let loaded = try await s.load(.local, header: nil)
    #expect(loaded.config.monitors.count == 1 && loaded.at == t0)
}

final class MockHeaders: HeaderStore, @unchecked Sendable {
    private let lock = NSLock()
    private var value: String?
    var header: String? { lock.withLock { value } }
    func setHeader(_ header: String) throws { lock.withLock { value = header.isEmpty ? nil : header } }
}

@Test func refreshAfterLoadSendsTheLoadsHeader() async throws {
    let t = MockTransport(replies: [url.absoluteString: .success((200, body, ["ETag": "\"e1\""]))])
    let s = ConfigStore(directory: .temporaryDirectory.appending(path: UUID().uuidString), transport: t, headers: MockHeaders())
    let source = ConfigSource.url(url, headerName: "Authorization")
    _ = try await s.load(source, header: "token x", now: t0)
    t.replies[url.absoluteString] = .success((304, "", [:]))
    let refreshed = try await s.refresh(source, now: t1)
    #expect(t.requests[1].0.value(forHTTPHeaderField: "Authorization") == "token x")
    #expect(refreshed.failure == nil && refreshed.at == t1)
}

@Test func failedRefreshSaysWhy() async throws {
    let t = MockTransport(replies: [url.absoluteString: .success((200, body, [:]))])
    let headers = MockHeaders()
    let s = ConfigStore(directory: .temporaryDirectory.appending(path: UUID().uuidString), transport: t, headers: headers)
    let source = ConfigSource.url(url, headerName: "Authorization")
    _ = try await s.load(source, header: "token x", now: t0)
    t.replies[url.absoluteString] = .success((404, "", [:]))
    #expect(try await s.refresh(source, now: t1).failure == "HTTP 404")
    t.replies[url.absoluteString] = .failure(URLError(.timedOut))
    let timedOut = try await s.refresh(source, now: t1)
    #expect(timedOut.failure == "timed out" && timedOut.at == t0)
    try headers.setHeader("")
    #expect(try await s.refresh(source, now: t1).failure == "no header")
    #expect(t.requests.count == 3, "no header sends no request")
}
