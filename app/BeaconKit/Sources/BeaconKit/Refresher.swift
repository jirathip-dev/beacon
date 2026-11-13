import Foundation

/// Each host's last `ETag` and body, kept in memory between refreshes (ADR D4).
public actor HostCache {
    private var entries: [URL: (etag: String, body: StatusBody)] = [:]
    public init() {}
    func get(_ url: URL) -> (etag: String, body: StatusBody)? { entries[url] }
    func set(_ url: URL, etag: String, body: StatusBody) { entries[url] = (etag, body) }
}

enum HostReply: Sendable {
    case body(StatusBody)
    case unreachable(String)
}

private enum Outcome: Sendable {
    case probe(Int, up: Bool, detail: String)
    case host(String, HostReply)
}

/// One refresh: every probe and one GET per host, concurrently in one task group (ADR D3, D4, D5).
public struct Refresher: Sendable {
    let transport: any Transport
    let hosts: HostCache

    public init(transport: any Transport = LiveTransport(), hosts: HostCache = HostCache()) {
        (self.transport, self.hosts) = (transport, hosts)
    }

    /// `timeoutCap` caps every timeout (10 s in `BGAppRefreshTask`). `previous` carries `since` over.
    public func run(
        _ config: MonitorsConfig, previous: [MonitorResult], online: Bool, timeoutCap: Int? = nil,
        now: Date = .now
    ) async -> [MonitorResult] {
        guard online else {
            return config.monitors.map {
                MonitorResult(name: $0.name, group: $0.group, state: .unknown, detail: "", since: nil)
            }
        }
        let cap = { @Sendable (t: Int) in min(t, timeoutCap ?? t) }
        let hostNames = Set(config.monitors.compactMap { m -> String? in
            if case .heartbeat(let h) = m { h.host } else { nil }
        })
        var probes: [Int: (up: Bool, detail: String)] = [:]
        var replies: [String: HostReply] = [:]
        await withTaskGroup(of: Outcome.self) { group in
            for name in hostNames {
                let host = config.hosts?[name]
                group.addTask { .host(name, await fetch(host, cap: cap)) }
            }
            for (i, monitor) in config.monitors.enumerated() {
                switch monitor {
                case .http(let m):
                    group.addTask {
                        let (up, detail) = await http(m, timeout: cap(m.timeout_s ?? 10))
                        return .probe(i, up: up, detail: detail)
                    }
                case .tcp(let m):
                    group.addTask {
                        let (up, detail) = await tcp(m, timeout: cap(m.timeout_s ?? 10))
                        return .probe(i, up: up, detail: detail)
                    }
                case .heartbeat: break
                }
            }
            for await outcome in group {
                switch outcome {
                case .probe(let i, let up, let detail): probes[i] = (up, detail)
                case .host(let name, let reply): replies[name] = reply
                }
            }
        }
        let before = Dictionary(previous.map { ($0.key, $0) }, uniquingKeysWith: { a, _ in a })
        return config.monitors.enumerated().map { i, monitor in
            let row = { (state: ResultState, detail: String, since: Date?) in
                MonitorResult(name: monitor.name, group: monitor.group, state: state, detail: detail, since: since)
            }
            let state: ResultState, detail: String
            switch monitor {
            case .heartbeat(let m):
                switch replies[m.host] ?? .unreachable("not checked") {
                case .body(let body):
                    guard let check = body.checks[m.id] else {
                        (state, detail) = (.down, "no result yet")
                        break
                    }
                    return row(check.status == .up ? .up : .down, check.message, check.since)
                case .unreachable(let reason): (state, detail) = (.down, "host unreachable: \(reason)")
                }
            default:
                let probe = probes[i] ?? (false, "not checked")
                (state, detail) = (probe.up ? .up : .down, probe.detail)
            }
            let old = before[row(state, detail, nil).key]
            return row(state, detail, old?.state == state ? old?.since ?? now : now)
        }
    }

    func fetch(_ host: Host?, cap: (Int) -> Int) async -> HostReply {
        guard let host else { return .unreachable("not in hosts") }
        let timeout = cap(host.timeout_s ?? 5)
        var request = URLRequest(url: host.url)
        let cached = await hosts.get(host.url)
        if let cached { request.setValue(cached.etag, forHTTPHeaderField: "If-None-Match") }
        do {
            let (data, response) = try await transport.get(request, timeout: timeout)
            if response.statusCode == 304, let cached { return .body(cached.body) }
            guard (200..<300).contains(response.statusCode) else {
                return .unreachable("HTTP \(response.statusCode)")
            }
            let body = try StatusBody.decode(data)
            if let etag = response.value(forHTTPHeaderField: "ETag") {
                await hosts.set(host.url, etag: etag, body: body)
            }
            return .body(body)
        } catch {
            return .unreachable(reason(error, timeout: timeout))
        }
    }

    func http(_ m: HTTPMonitor, timeout: Int) async -> (Bool, String) {
        do {
            let (data, response) = try await transport.get(URLRequest(url: m.url), timeout: timeout)
            let code = response.statusCode
            guard m.expect_status.map({ $0 == code }) ?? (200..<300).contains(code) else {
                return (false, "HTTP \(code)")
            }
            if let keyword = m.keyword, !String(decoding: data, as: UTF8.self).contains(keyword) {
                return (false, "keyword missing: \(keyword)")
            }
            return (true, "")
        } catch {
            return (false, reason(error, timeout: timeout))
        }
    }

    func tcp(_ m: TCPMonitor, timeout: Int) async -> (Bool, String) {
        do {
            try await transport.connect(host: m.host, port: m.port, timeout: timeout)
            return (true, "")
        } catch {
            return (false, reason(error, timeout: timeout))
        }
    }
}

private func reason(_ error: any Error, timeout: Int) -> String {
    (error as? URLError)?.code == .timedOut ? "timeout after \(timeout)s" : describe(error)
}

extension MonitorResult {
    var key: String { "\(group)/\(name)" }
}

public struct Alert: Sendable {
    public let title: String
    public let body: String
}

/// One alert per monitor that went up → down, and one per monitor that went down → up (q1579).
public func alerts(from old: [MonitorResult], to new: [MonitorResult]) -> [Alert] {
    let before = Dictionary(old.map { ($0.key, $0.state) }, uniquingKeysWith: { a, _ in a })
    return new.compactMap { row in
        switch (before[row.key], row.state) {
        case (.up, .down): Alert(title: "\(row.name) down", body: row.detail)
        case (.down, .up): Alert(title: "\(row.name) recovered", body: row.group)
        default: nil
        }
    }
}
