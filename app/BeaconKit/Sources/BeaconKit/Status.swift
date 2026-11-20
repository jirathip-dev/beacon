import Foundation

/// `beacon serve`'s `GET /status` body (status.schema.json); unknown fields are ignored (ADR D1).
public struct StatusBody: Decodable, Sendable {
    public let version: Int
    public let host: String
    public let checks: [String: CheckResult]

    public static func decode(_ data: Data) throws -> StatusBody {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let body = try decoder.decode(StatusBody.self, from: data)
        guard body.version == 1 else { throw BeaconError("unsupported version \(body.version)") }
        return body
    }
}

public struct CheckResult: Decodable, Sendable {
    public let status: Status
    public let message: String
    public let since: Date
    /// Only on a job's row (ADR D12); a row with a name and no group is unreviewed.
    public let name: String?
    public let group: String?
}

public enum Status: String, Decodable, Sendable { case up, down }

/// unknown: the phone has no network path, so nothing is known to be down (ADR D5).
public enum ResultState: String, Codable, Sendable { case up, down, unknown }

public struct MonitorResult: Codable, Sendable, Equatable {
    public let name: String
    public let group: String
    public let state: ResultState
    public let detail: String
    public let since: Date?
    /// The serve host of a job row.
    public let host: String?

    public init(name: String, group: String, state: ResultState, detail: String, since: Date?, host: String? = nil) {
        (self.name, self.group, self.state, self.detail, self.since, self.host) = (name, group, state, detail, since, host)
    }

    public static let unreviewed = "Unreviewed"
}

/// What the app saves after each refresh and the widget reads (ADR D8).
public struct StatusSnapshot: Codable, Sendable {
    public let rows: [MonitorResult]
    public let at: Date
    public let configAt: Date
    /// Why the config fetch failed; `configAt` is then the cached copy's time (ADR D2).
    public let configFailure: String?

    public init(rows: [MonitorResult], at: Date, configAt: Date, configFailure: String?) {
        (self.rows, self.at, self.configAt, self.configFailure) = (rows, at, configAt, configFailure)
    }

    public var down: [MonitorResult] { rows.filter { $0.state == .down } }
    public var upCount: Int { rows.count { $0.state == .up } }
    public var offline: Bool { !rows.isEmpty && rows.allSatisfy { $0.state == .unknown } }

    public static let appGroup = "group.dev.jirathip.beacon"
    public static var directory: URL {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: appGroup)
            ?? URL.applicationSupportDirectory
    }
    static var file: URL { directory.appending(path: "snapshot.json") }

    public static func load() -> StatusSnapshot? {
        (try? Data(contentsOf: file)).flatMap { try? JSONDecoder().decode(StatusSnapshot.self, from: $0) }
    }

    public func save() throws {
        try FileManager.default.createDirectory(at: Self.directory, withIntermediateDirectories: true)
        try JSONEncoder().encode(self).write(to: Self.file, options: .atomic)
    }
}

public struct BeaconError: LocalizedError, Sendable {
    public let message: String
    public init(_ message: String) { self.message = message }
    public var errorDescription: String? { message }
}

/// A decode error as `monitors[2].type: unknown monitor type ping`, for the config form.
public func describe(_ error: any Error) -> String {
    let context: DecodingError.Context
    switch error as? DecodingError {
    case .dataCorrupted(let c), .typeMismatch(_, let c), .valueNotFound(_, let c): context = c
    case .keyNotFound(let key, let c):
        return "\(path(c.codingPath + [key])): missing"
    default: return error.localizedDescription
    }
    let at = path(context.codingPath)
    return at.isEmpty ? context.debugDescription : "\(at): \(context.debugDescription)"
}

private func path(_ keys: [any CodingKey]) -> String {
    keys.map { $0.intValue.map { "[\($0)]" } ?? ".\($0.stringValue)" }.joined()
        .trimmingCharacters(in: ["."])
}
