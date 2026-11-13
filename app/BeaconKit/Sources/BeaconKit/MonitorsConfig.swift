import Foundation

public struct MonitorsConfig: Decodable, Sendable {
    public let version: Int
    public let monitors: [Monitor]
    public let hosts: [String: Host]?

    enum CodingKeys: String, CodingKey { case version, monitors, hosts }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        version = try c.decode(Int.self, forKey: .version)
        guard version == 1 else {
            throw DecodingError.dataCorruptedError(
                forKey: .version, in: c, debugDescription: "unsupported version \(version)")
        }
        monitors = try c.decode([Monitor].self, forKey: .monitors)
        hosts = try c.decodeIfPresent([String: Host].self, forKey: .hosts)
    }
}

public enum Monitor: Decodable, Sendable {
    case http(HTTPMonitor), tcp(TCPMonitor), heartbeat(HeartbeatMonitor)

    enum CodingKeys: String, CodingKey { case type }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        switch try c.decode(String.self, forKey: .type) {
        case "http": self = .http(try HTTPMonitor(from: decoder))
        case "tcp": self = .tcp(try TCPMonitor(from: decoder))
        case "heartbeat": self = .heartbeat(try HeartbeatMonitor(from: decoder))
        case let type:
            throw DecodingError.dataCorruptedError(
                forKey: .type, in: c, debugDescription: "unknown monitor type \(type)")
        }
    }
}

public struct HTTPMonitor: Decodable, Sendable {
    public let name: String
    public let group: String
    public let url: URL
    public let expect_status: Int?
    public let keyword: String?
    public let timeout_s: Int?
}

public struct TCPMonitor: Decodable, Sendable {
    public let name: String
    public let group: String
    public let host: String
    public let port: Int
    public let timeout_s: Int?
}

public struct HeartbeatMonitor: Decodable, Sendable {
    public let name: String
    public let group: String
    public let host: String
    public let id: String
}

public struct Host: Decodable, Sendable {
    public let url: URL
    public let timeout_s: Int?
}

extension Monitor {
    public var name: String {
        switch self {
        case .http(let m): m.name
        case .tcp(let m): m.name
        case .heartbeat(let m): m.name
        }
    }

    public var group: String {
        switch self {
        case .http(let m): m.group
        case .tcp(let m): m.group
        case .heartbeat(let m): m.group
        }
    }
}
