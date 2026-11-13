import Foundation

/// Where the config comes from (ADR D2). The header value lives in the Keychain, never here.
public enum ConfigSource: Codable, Sendable, Equatable {
    case url(URL, headerName: String?)
    /// The copy imported from Files, kept in the store's directory.
    case local
}

public struct LoadedConfig: Sendable {
    public let config: MonitorsConfig
    public let at: Date
    /// The fetch failed and this is the cached copy (shown as "config from <at>").
    public let stale: Bool
}

/// The config file and its `ETag`, cached in the App Group container (ADR D2).
public struct ConfigStore: Sendable {
    struct Meta: Codable {
        let url: URL?
        let etag: String?
        let at: Date
    }

    let directory: URL
    let transport: any Transport

    public init(directory: URL = StatusSnapshot.directory, transport: any Transport = LiveTransport()) {
        (self.directory, self.transport) = (directory, transport)
    }

    var file: URL { directory.appending(path: "monitors.json") }
    var metaFile: URL { directory.appending(path: "monitors.meta.json") }

    /// Decodes an imported file and keeps it as the `.local` copy; an invalid file throws its decode error.
    public func importLocal(_ data: Data, now: Date = .now) throws -> LoadedConfig {
        let config = try JSONDecoder().decode(MonitorsConfig.self, from: data)
        try save(data, Meta(url: nil, etag: nil, at: now))
        return LoadedConfig(config: config, at: now, stale: false)
    }

    /// `.url`: a conditional GET; a 304 or a failed fetch uses the cached copy of that same URL.
    public func load(_ source: ConfigSource, header: String?, now: Date = .now) async throws -> LoadedConfig {
        let meta = (try? Data(contentsOf: metaFile)).flatMap { try? JSONDecoder().decode(Meta.self, from: $0) }
        guard case .url(let url, let headerName) = source else {
            let config = try JSONDecoder().decode(MonitorsConfig.self, from: Data(contentsOf: file))
            return LoadedConfig(config: config, at: meta?.at ?? now, stale: false)
        }
        let cached = meta?.url == url ? meta : nil
        var request = URLRequest(url: url)
        if let headerName, let header { request.setValue(header, forHTTPHeaderField: headerName) }
        if let etag = cached?.etag { request.setValue(etag, forHTTPHeaderField: "If-None-Match") }
        do {
            let (data, response) = try await transport.get(request, timeout: 10)
            if response.statusCode == 304, cached != nil {
                let config = try JSONDecoder().decode(MonitorsConfig.self, from: Data(contentsOf: file))
                try save(nil, Meta(url: url, etag: cached?.etag, at: now))
                return LoadedConfig(config: config, at: now, stale: false)
            }
            guard (200..<300).contains(response.statusCode) else { throw BeaconError("HTTP \(response.statusCode)") }
            let config = try JSONDecoder().decode(MonitorsConfig.self, from: data)
            try save(data, Meta(url: url, etag: response.value(forHTTPHeaderField: "ETag"), at: now))
            return LoadedConfig(config: config, at: now, stale: false)
        } catch {
            guard let cached, let data = try? Data(contentsOf: file),
                let config = try? JSONDecoder().decode(MonitorsConfig.self, from: data)
            else { throw error }
            return LoadedConfig(config: config, at: cached.at, stale: true)
        }
    }

    private func save(_ data: Data?, _ meta: Meta) throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try data?.write(to: file, options: .atomic)
        try JSONEncoder().encode(meta).write(to: metaFile, options: .atomic)
    }
}
