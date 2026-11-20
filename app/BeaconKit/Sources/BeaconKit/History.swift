import Foundation

/// `beacon serve`'s `GET /history` body (history.schema.json): every key's incidents and the unwatched gaps.
public struct HistoryBody: Decodable, Sendable {
    public let version: Int
    public let host: String
    public let gaps: [[Int]]
    public let checks: [String: Track]
    public let monitors: [MonitorTrack]

    public static func decode(_ data: Data) throws -> HistoryBody {
        let body = try JSONDecoder().decode(HistoryBody.self, from: data)
        guard body.version == 1 else { throw BeaconError("unsupported version \(body.version)") }
        return body
    }

    public func track(of monitor: Monitor) -> MonitorTrack? {
        monitors.first { $0.group == monitor.group && $0.name == monitor.name }
    }
}

public struct Track: Decodable, Sendable {
    public let since: Int
    public let down: [Incident]

    /// Over the last `w` seconds to `now`: the span known, the seconds watched (not in a gap) and the watched seconds down.
    public func tally(last w: Int, gaps: [[Int]], now: Int) -> (span: Int, watched: Int, down: Int) {
        let from = max(now - w, since)
        let overlap = { (a: Int, b: Int, c: Int, d: Int) in max(0, min(b, d) - max(a, c)) }
        let unseen = { (a: Int, b: Int) in gaps.reduce(0) { $0 + overlap($1.first ?? 0, $1.last ?? 0, a, b) } }
        let down = down.reduce(0) { sum, i in
            let (a, b) = (max(i.down_at, from), min(i.up_at ?? now, now))
            return sum + (b > a ? b - a - unseen(a, b) : 0)
        }
        return (max(0, now - from), max(0, now - from - unseen(from, now)), down)
    }

    /// In 0...1; nil when nothing in the window was watched.
    public func uptime(last w: Int, gaps: [[Int]], now: Int) -> Double? {
        let t = tally(last: w, gaps: gaps, now: now)
        return t.watched > 0 ? 1 - Double(t.down) / Double(t.watched) : nil
    }
}

public struct Incident: Decodable, Sendable, Hashable {
    public let down_at: Int
    public let up_at: Int?
    public let message: String
}

public struct MonitorTrack: Decodable, Sendable {
    public let id, group, name: String
    public let track: Track

    enum CodingKeys: String, CodingKey { case id, group, name }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        (id, group, name) = try (c.decode(String.self, forKey: .id), c.decode(String.self, forKey: .group),
                                 c.decode(String.self, forKey: .name))
        track = try Track(from: decoder)
    }
}

/// `GET /history/<id>` (times.schema.json): one monitor's probes for 24 h and its hours for 30 d.
public struct TimesBody: Decodable, Sendable {
    public let version: Int
    public let id: String
    public let every_s: Int
    public let recent: Recent
    public let hourly: Hourly

    public struct Recent: Decodable, Sendable { public let t: [Int]; public let ms: [Double?] }
    public struct Hourly: Decodable, Sendable { public let t: [Int]; public let avg_ms, max_ms: [Double?] }

    public static func decode(_ data: Data) throws -> TimesBody {
        let b = try JSONDecoder().decode(TimesBody.self, from: data)
        guard b.version == 1 else { throw BeaconError("unsupported version \(b.version)") }
        guard b.recent.ms.count == b.recent.t.count, b.hourly.avg_ms.count == b.hourly.t.count,
              b.hourly.max_ms.count == b.hourly.t.count else { throw BeaconError("times columns differ in length") }
        return b
    }
}
