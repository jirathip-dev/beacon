import BackgroundTasks
import BeaconKit
import Security
import SwiftUI
import UserNotifications
import WidgetKit

let refreshTask = "dev.jirathip.beacon.refresh"

@main
struct BeaconApp: App {
    @State private var model = Model()
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            Root(model: model)
                .task { _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) }
        }
        .onChange(of: phase) { if phase == .background { scheduleRefresh() } }
        .backgroundTask(.appRefresh(refreshTask)) {
            scheduleRefresh()
            await model.refresh(background: true)
        }
    }
}

/// As often as iOS allows: the earliest date is now (ADR D8).
func scheduleRefresh() {
    try? BGTaskScheduler.shared.submit(BGAppRefreshTaskRequest(identifier: refreshTask))
}

@MainActor @Observable final class Model {
    private(set) var source = savedSource()
    private(set) var snapshot = StatusSnapshot.load()
    /// The monitor count while the first check after a config change is out.
    private(set) var checking: Int?
    private(set) var config: MonitorsConfig?
    let store = ConfigStore()
    private let refresher = Refresher()
    private var inFlight: Task<Void, Never>?

    /// Makes `source` current once its config has loaded; the caller shows a thrown error as "refused".
    func use(_ source: URL, loaded: LoadedConfig) {
        self.source = source
        UserDefaults.standard.set(ConfigSource.data(source), forKey: "source")
        snapshot = nil
        checking = loaded.config.monitors.count
        Task {
            await inFlight?.value
            await refresh()
        }
    }

    /// The serve host a list row reads: a heartbeat whose host is in `hosts`.
    func host(of row: MonitorResult) -> String? {
        for case .heartbeat(let m) in config?.monitors ?? [] where m.name == row.name && m.group == row.group {
            return config?.hosts?[m.host] == nil ? nil : m.host
        }
        return nil
    }

    func refresh(background: Bool = false) async {
        if let inFlight { return await inFlight.value }
        let task = Task { await run(background: background) }
        inFlight = task
        await task.value
        inFlight = nil
    }

    private func run(background: Bool) async {
        guard let source else { return }
        let previous = snapshot?.rows ?? []
        let online = await networkAvailable()
        let next: StatusSnapshot
        do {
            let loaded = try await store.load(source)
            config = loaded.config
            let rows = await refresher.run(
                loaded.config, previous: previous, online: online, timeoutCap: background ? 10 : nil)
            next = StatusSnapshot(rows: rows, at: .now, configAt: loaded.at, configFailure: loaded.failure)
        } catch {
            let row = MonitorResult(
                name: "config", group: "config", state: online ? .down : .unknown, detail: describe(error), since: nil)
            next = StatusSnapshot(rows: [row], at: .now, configAt: .now, configFailure: nil)
        }
        try? next.save()
        snapshot = next
        checking = nil
        WidgetCenter.shared.reloadAllTimelines()
        guard background else { return }
        for alert in alerts(from: previous, to: next.rows) {
            let content = UNMutableNotificationContent()
            (content.title, content.body) = (alert.title, alert.body)
            try? await UNUserNotificationCenter.current()
                .add(UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil))
        }
    }
}

/// The saved config URL; an older build's header or `.local` source is dropped, and its Keychain header with it.
func savedSource() -> URL? {
    let header: [CFString: Any] = [kSecClass: kSecClassGenericPassword, kSecAttrService: "dev.jirathip.beacon", kSecAttrAccount: "config-header"]
    SecItemDelete(header as CFDictionary)
    guard let url = UserDefaults.standard.data(forKey: "source").flatMap(ConfigSource.url(from:)) else {
        UserDefaults.standard.removeObject(forKey: "source")
        return nil
    }
    return url
}
