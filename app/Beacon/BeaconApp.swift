import BackgroundTasks
import BeaconKit
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
    private(set) var source: ConfigSource? = UserDefaults.standard.data(forKey: "source")
        .flatMap { try? JSONDecoder().decode(ConfigSource.self, from: $0) }
    private(set) var snapshot = StatusSnapshot.load()
    /// The monitor count while the first check after a config change is out.
    private(set) var checking: Int?
    let store = ConfigStore()
    private let refresher = Refresher()
    private var inFlight: Task<Void, Never>?

    /// Makes `source` current once its config has loaded; the caller shows a thrown error as "refused".
    func use(_ source: ConfigSource, loaded: LoadedConfig) {
        self.source = source
        UserDefaults.standard.set(try? JSONEncoder().encode(source), forKey: "source")
        snapshot = nil
        checking = loaded.config.monitors.count
        Task {
            await inFlight?.value
            await refresh()
        }
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
        let header = source == .local ? nil : Keychain.header
        let next: StatusSnapshot
        do {
            let loaded = try await store.load(source, header: header)
            let rows = await refresher.run(
                loaded.config, previous: previous, online: online, timeoutCap: background ? 10 : nil)
            next = StatusSnapshot(rows: rows, at: .now, configAt: loaded.at, configStale: loaded.stale)
        } catch {
            let row = MonitorResult(
                name: "config", group: "config", state: online ? .down : .unknown, detail: describe(error), since: nil)
            next = StatusSnapshot(rows: [row], at: .now, configAt: .now, configStale: false)
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
