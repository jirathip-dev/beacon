import Foundation

/// The saved config URL, kept in the older builds' `.url` JSON; a source with a header or the `.local` copy reads as nil.
public enum ConfigSource {
    public static func url(from data: Data) -> URL? {
        let saved = (try? JSONSerialization.jsonObject(with: data)) as? [String: [String: String]]
        guard let fields = saved?["url"], fields["headerName"] == nil else { return nil }
        return fields["_0"].flatMap { URL(string: $0) }
    }

    public static func data(_ url: URL) -> Data {
        (try? JSONSerialization.data(withJSONObject: ["url": ["_0": url.absoluteString]])) ?? Data()
    }
}

public struct LoadedConfig: Sendable {
    public let config: MonitorsConfig
    public let at: Date
    /// Why the fetch failed, when this is the cached copy (shown as "config from <at> · <failure>").
    public let failure: String?
}

/// The config file and its `ETag`, cached in the App Group container (ADR D2).
public struct ConfigStore: Sendable {
    struct Meta: Codable {
        let url: URL
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

    /// Connect and refresh: a conditional GET; a 304 or a failed fetch uses the cached copy of that same URL.
    public func load(_ url: URL, now: Date = .now) async throws -> LoadedConfig {
        let meta = (try? Data(contentsOf: metaFile)).flatMap { try? JSONDecoder().decode(Meta.self, from: $0) }
        let cached = meta?.url == url ? meta : nil
        var request = URLRequest(url: url)
        if let etag = cached?.etag { request.setValue(etag, forHTTPHeaderField: "If-None-Match") }
        do {
            let (data, response) = try await transport.get(request, timeout: 10)
            if response.statusCode == 304, cached != nil {
                let config = try JSONDecoder().decode(MonitorsConfig.self, from: Data(contentsOf: file))
                try save(nil, Meta(url: url, etag: cached?.etag, at: now))
                return LoadedConfig(config: config, at: now, failure: nil)
            }
            guard (200..<300).contains(response.statusCode) else { throw BeaconError("HTTP \(response.statusCode)") }
            let config = try JSONDecoder().decode(MonitorsConfig.self, from: data)
            try save(data, Meta(url: url, etag: response.value(forHTTPHeaderField: "ETag"), at: now))
            return LoadedConfig(config: config, at: now, failure: nil)
        } catch {
            let why = reason(error)
            guard let cached, let data = try? Data(contentsOf: file),
                let config = try? JSONDecoder().decode(MonitorsConfig.self, from: data)
            else { throw BeaconError(why) }
            return LoadedConfig(config: config, at: cached.at, failure: why)
        }
    }

    func reason(_ error: any Error) -> String {
        let offline: [URLError.Code] = [
            .timedOut, .cannotFindHost, .cannotConnectToHost, .notConnectedToInternet, .networkConnectionLost,
            .dnsLookupFailed,
        ]
        if let code = (error as? URLError)?.code, offline.contains(code) { return "can't reach the Mini: is Tailscale on?" }
        return error is DecodingError ? "not a Beacon config: \(describe(error))" : describe(error)
    }

    private func save(_ data: Data?, _ meta: Meta) throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try data?.write(to: file, options: .atomic)
        try JSONEncoder().encode(meta).write(to: metaFile, options: .atomic)
    }
}
