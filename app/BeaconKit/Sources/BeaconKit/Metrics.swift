import Foundation

/// `beacon serve`'s `GET /metrics` body (metrics.schema.json); unknown fields are ignored (ADR D1).
public struct MetricsBody: Decodable, Sendable {
    public let version: Int
    public let host: String
    public let every_s: Int
    public let t: [Int]
    public let load1, load5, load15, cpu_pct: [Double?]
    public let mem, swap: Gauge
    public let disks: [String: Disk]

    public struct Gauge: Decodable, Sendable {
        public let total_bytes: Int?
        public let used_bytes: [Int?]
    }

    public struct Disk: Decodable, Sendable {
        public let used_bytes, free_bytes: [Int?]
    }

    public static func decode(_ data: Data) throws -> MetricsBody {
        let body = try JSONDecoder().decode(MetricsBody.self, from: data)
        guard body.version == 1 else { throw BeaconError("unsupported version \(body.version)") }
        let counts = [body.load1.count, body.load5.count, body.load15.count, body.cpu_pct.count,
                      body.mem.used_bytes.count, body.swap.used_bytes.count]
            + body.disks.values.flatMap { [$0.used_bytes.count, $0.free_bytes.count] }
        guard counts.allSatisfy({ $0 == body.t.count }) else { throw BeaconError("metrics columns differ in length") }
        return body
    }

    /// The mounts in the host's `disks` order; any others after, sorted.
    public func mounts(order: [String]?) -> [String] {
        let known = (order ?? []).filter { disks[$0] != nil }
        return known + disks.keys.filter { !known.contains($0) }.sorted()
    }

    /// load, cpu, mem, then one disk per mount: each value lines up with `t`.
    public func series(order: [String]?) -> [Series] {
        let pct = { (used: Int?, total: Int?) -> Double? in
            guard let used, let total, total > 0 else { return nil }
            return 100 * Double(used) / Double(total)
        }
        return [
            Series(id: "load", values: load1, percent: false),
            Series(id: "cpu", values: cpu_pct, percent: true),
            Series(id: "mem", values: mem.used_bytes.map { pct($0, mem.total_bytes) }, percent: true),
        ] + mounts(order: order).map { m in
            let d = disks[m]!
            return Series(id: "disk \(m)", values: zip(d.used_bytes, d.free_bytes).map { u, f in
                pct(u, u.flatMap { u in f.map { u + $0 } })
            }, percent: true)
        }
    }
}

/// One chartable column of a `MetricsBody`; `percent` fixes its y range at 0–100.
public struct Series: Sendable, Identifiable {
    public let id: String
    public let values: [Double?]
    public let percent: Bool

    public var now: Double? { values.last { $0 != nil } ?? nil }

    /// The y range's top: 100 for a percentage, else a round number at or above the peak.
    public func top(_ ys: [Double?]) -> Double {
        if percent { return 100 }
        let peak = max(1, ys.compactMap { $0 }.max() ?? 1)
        return [1, 2, 4, 5, 8, 10, 16, 20, 40, 50, 100].first { $0 >= peak } ?? peak.rounded(.up)
    }
}

/// A chart range: the samples of its last `seconds`, plotted as means of `bucket` samples each.
public enum ChartRange: String, CaseIterable, Sendable {
    case hour = "1h", six = "6h", day = "24h"

    public var seconds: Int { ["1h": 3600, "6h": 6 * 3600, "24h": 24 * 3600][rawValue]! }
    public var bucket: Int { self == .day ? 4 : 1 }
}

/// What a chart draws for one series over one range; min, avg and max come from the raw samples.
public struct Window: Sendable {
    public let t: [Int]
    public let values: [Double?]
    public let points: [(t: Int, y: Double?)]

    public init(_ t: [Int], _ values: [Double?], range: ChartRange) {
        let from = (t.last ?? 0) - range.seconds
        let start = t.firstIndex { $0 > from } ?? t.count
        let (t, values) = (Array(t[start...]), Array(values[start...]))
        (self.t, self.values) = (t, values)
        points = stride(from: 0, to: t.count, by: range.bucket).map { i in
            let end = Swift.min(i + range.bucket, t.count)
            let ys = values[i..<end].compactMap { $0 }
            return (t[end - 1], ys.isEmpty ? nil : ys.reduce(0, +) / Double(ys.count))
        }
    }

    public var present: [Double] { values.compactMap { $0 } }
    public var missing: Int { values.count - present.count }
    public var min: Double? { present.min() }
    public var max: Double? { present.max() }
    public var avg: Double? { present.isEmpty ? nil : present.reduce(0, +) / Double(present.count) }
}

public enum MetricsReply: Sendable {
    case fresh(MetricsBody, etag: String?)
    case unchanged
    /// 404: the host's serve is from before `/metrics`.
    case notServed
    case unreachable(String)
}

/// One `GET /metrics`, only while the host detail screen is open (ADR D10).
public struct HostMetrics: Sendable {
    let transport: any Transport

    public init(_ transport: any Transport = LiveTransport()) { self.transport = transport }

    public func fetch(_ url: URL, etag: String?, timeout: Int) async -> MetricsReply {
        var request = URLRequest(url: url)
        if let etag { request.setValue(etag, forHTTPHeaderField: "If-None-Match") }
        do {
            let (data, response) = try await transport.get(request, timeout: timeout)
            switch response.statusCode {
            case 304 where etag != nil: return .unchanged
            case 404: return .notServed
            case 200..<300:
                return .fresh(try MetricsBody.decode(data), etag: response.value(forHTTPHeaderField: "ETag"))
            case let code: return .unreachable("HTTP \(code)")
            }
        } catch {
            return .unreachable(reason(error, timeout: timeout))
        }
    }
}

extension Host {
    /// The status URL with `metrics` as its last segment.
    public var metricsURL: URL { URL(string: "metrics", relativeTo: url)!.absoluteURL }
}
