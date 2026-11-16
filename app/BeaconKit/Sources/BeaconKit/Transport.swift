import Foundation
import Network

/// The only network the probes and the config store use; tests pass a mock.
public protocol Transport: Sendable {
    /// One GET with a total timeout. A timeout throws `URLError(.timedOut)`.
    func get(_ request: URLRequest, timeout: Int) async throws -> (Data, HTTPURLResponse)
    /// Completes once a TCP connection is ready. A timeout throws `URLError(.timedOut)`.
    func connect(host: String, port: Int, timeout: Int) async throws
}

public struct LiveTransport: Transport {
    public init() {}

    public func get(_ request: URLRequest, timeout: Int) async throws -> (Data, HTTPURLResponse) {
        let config = URLSessionConfiguration.ephemeral
        config.urlCache = nil
        config.requestCachePolicy = .reloadIgnoringLocalCacheData
        config.timeoutIntervalForRequest = TimeInterval(timeout)
        config.timeoutIntervalForResource = TimeInterval(timeout)
        let session = URLSession(configuration: config)
        defer { session.finishTasksAndInvalidate() }
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else { throw URLError(.badServerResponse) }
        return (data, http)
    }

    public func connect(host: String, port: Int, timeout: Int) async throws {
        guard let port = NWEndpoint.Port(rawValue: UInt16(clamping: port)) else { throw URLError(.badURL) }
        let connection = NWConnection(host: NWEndpoint.Host(host), port: port, using: .tcp)
        let once = Once()
        try await withCheckedThrowingContinuation { (done: CheckedContinuation<Void, any Error>) in
            let finish: @Sendable ((any Error)?) -> Void = { error in
                guard once.claim() else { return }
                connection.cancel()
                if let error { done.resume(throwing: error) } else { done.resume() }
            }
            connection.stateUpdateHandler = { state in
                switch state {
                case .ready: finish(nil)
                case .failed(let e), .waiting(let e): finish(e)
                default: break
                }
            }
            connection.start(queue: .global())
            DispatchQueue.global().asyncAfter(deadline: .now() + .seconds(timeout)) {
                finish(URLError(.timedOut))
            }
        }
    }
}

private final class Once: @unchecked Sendable {
    private let lock = NSLock()
    private var done = false
    func claim() -> Bool { lock.withLock { defer { done = true }; return !done } }
}

/// One look at the network path: false when `NWPathMonitor` reports it unsatisfied (ADR D5).
public func networkAvailable() async -> Bool {
    let monitor = NWPathMonitor()
    defer { monitor.cancel() }
    return await withCheckedContinuation { done in
        let once = Once()
        monitor.pathUpdateHandler = { path in
            if once.claim() { done.resume(returning: path.status == .satisfied) }
        }
        monitor.start(queue: .global())
    }
}
