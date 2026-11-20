import BeaconKit
import SwiftUI

struct Root: View {
    let model: Model
    @State private var settings = false
    @State private var host: String?
    @State private var history: (HistoryTarget, Monitor?)?
    @Environment(\.scenePhase) private var phase

    var body: some View {
        if model.source == nil, model.snapshot == nil {
            start
        } else {
            Group {
                if let history {
                    HistoryScreen(model: model, target: history.0, monitor: history.1) { self.history = nil }
                } else if let host, let h = model.config?.hosts?[host] {
                    HostDetail(model: model, name: host, host: h) { self.host = nil } open: { history = ($0, nil) }
                } else {
                    MonitorList(model: model, openSettings: { settings = true }) { row in
                        if let h = model.host(of: row) { return { host = h } }
                        return model.history(of: row).map { t in { history = (t, model.monitor(of: row)) } }
                    }
                }
            }
                .task(id: phase) {
                    while phase == .active, !Task.isCancelled {
                        await model.refresh()
                        try? await Task.sleep(for: .seconds(60))
                    }
                }
                .sheet(isPresented: Binding { settings || model.source == nil } set: { settings = $0 }) {
                    Group {
                        if model.source == nil {
                            start.interactiveDismissDisabled()
                        } else {
                            VStack(spacing: 0) {
                                TopBar(title: "config", key: "done") { settings = false }
                                SourceForm(model: model, current: true) { settings = false }
                            }
                        }
                    }
                    .background(T.bg)
                    .presentationBackground(T.bg)
                }
        }
    }

    var start: some View {
        VStack(spacing: 0) {
            TopBar(title: "beacon")
            SourceForm(model: model)
        }
        .background(T.bg)
    }
}

/// A lowercase `[key]` in the accent colour, with a 44 pt hit area whatever its text width.
struct Key: View {
    let title: String
    var enabled = true
    let action: () -> Void
    var body: some View {
        Button {
            if enabled { action() }
        } label: {
            Text("[\(title)]")
                .font(T.font())
                .foregroundStyle(enabled ? T.accent : T.mute)
                .frame(minWidth: T.hit, minHeight: T.hit)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .allowsHitTesting(enabled)
    }
}

struct TopBar: View {
    let title: String
    var key: String? = nil
    var action: () -> Void = {}
    var body: some View {
        HStack(spacing: 0) {
            Text(title).font(T.font(15, .bold)).foregroundStyle(T.fg)
            Spacer()
            if let key { Key(title: key, action: action) }
        }
        .frame(minHeight: T.hit)
        .padding(.leading, 16).padding(.trailing, 10)
        .overlay(alignment: .bottom) { T.line.frame(height: 1) }
    }
}

/// A rounded card with Cooee's 1 pt edge and an optional coloured top rule.
struct Card<Content: View>: View {
    var rule: Color? = nil
    @ViewBuilder let content: Content
    var body: some View {
        VStack(alignment: .leading, spacing: 0) { content }
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(T.card)
            .overlay(alignment: .top) { if let rule { rule.frame(height: 3) } }
            .clipShape(RoundedRectangle(cornerRadius: T.radius))
            .overlay(RoundedRectangle(cornerRadius: T.radius).stroke(T.edge, lineWidth: 1))
    }
}

struct MonitorRow: View {
    let row: MonitorResult
    /// true: a mute `›` for a row that opens its host; false: the same width blank; nil: neither.
    var chevron: Bool? = nil
    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 8) {
                Dot(color: T.color(row.state))
                Text(row.name).foregroundStyle(T.fg).lineLimit(1).truncationMode(.tail)
                Spacer(minLength: 8)
                HStack(spacing: 0) {
                    Text(row.state == .unknown ? "--" : row.state.rawValue).foregroundStyle(T.color(row.state))
                    if row.state != .unknown, let since = row.since {
                        Text(" since \(T.stamp(since))").foregroundStyle(T.mute)
                    }
                }
                .font(T.font(11)).fixedSize()
                if let chevron {
                    Text("›").foregroundStyle(T.mute).frame(width: 22, alignment: .trailing).opacity(chevron ? 1 : 0)
                }
            }
            if row.state == .down, !row.detail.isEmpty {
                Text(row.detail).font(T.font(11)).foregroundStyle(T.mute)
                    .lineLimit(1).truncationMode(.tail)
                    .padding(.leading, 15)
            }
        }
        .font(T.font())
        .padding(.horizontal, 12).padding(.vertical, 7)
        .frame(maxWidth: .infinity, alignment: .leading)
        .overlay(alignment: .leading) { if row.state == .down { T.red.frame(width: 3) } }
    }
}

struct GroupCard: View {
    let name: String
    let rows: [MonitorResult]
    var ruled = true
    /// The list's rows: a host-backed row opens its host; nil on the host detail.
    var open: ((MonitorResult) -> (() -> Void)?)? = nil
    var body: some View {
        let down = rows.count { $0.state == .down }
        let unknown = rows.allSatisfy { $0.state == .unknown }
        Card(rule: !ruled ? nil : unknown ? T.mute : down > 0 ? T.red : T.green) {
            let count = Group {
                if unknown { Text("\(rows.count) unknown").foregroundStyle(T.mute) }
                else if down > 0 { Text("\(down) down").foregroundStyle(T.red) }
                else { Text("\(rows.count) up").foregroundStyle(T.mute) }
            }
            if ruled {
                HStack(spacing: 10) {
                    Text("~ \(name.lowercased())").font(T.font(13, .bold)).foregroundStyle(T.fg)
                    count.font(T.font(11))
                }
                .padding(.horizontal, 12).padding(.top, 11).padding(.bottom, 5)
            } else {
                HStack { Text("~ \(name.lowercased())").foregroundStyle(T.fg); Spacer(); count }
                    .padding(.horizontal, 12).padding(.vertical, 8)
                T.line.frame(height: 1)
            }
            ForEach(Array(rows.enumerated()), id: \.offset) { i, row in
                if i > 0 { T.line.frame(height: 1).padding(.leading, 12) }
                let action = open?(row)
                MonitorRow(row: row, chevron: open.map { _ in action != nil })
                    .contentShape(Rectangle())
                    .onTapGesture { action?() }
            }
            Spacer().frame(height: 4)
        }
    }
}

/// A centred card for a list with nothing to show yet, in Cooee's "phrase · plain words" form.
struct StateCard: View {
    var glyph: String? = nil
    let phrase: String
    let plain: String
    var key: (String, () -> Void)? = nil
    var body: some View {
        Card {
            VStack(spacing: 10) {
                if let glyph { Text(glyph).font(T.font(28)).foregroundStyle(T.mute) }
                HStack(spacing: 0) {
                    Text(phrase).foregroundStyle(T.fg)
                    Text(" · \(plain)").font(T.font(11)).foregroundStyle(T.mute)
                }
                if let key { Key(title: key.0, action: key.1) }
            }
            .font(T.font())
            .frame(maxWidth: .infinity)
            .padding(.vertical, key == nil ? 28 : 18)
        }
    }
}

struct MonitorList: View {
    let model: Model
    let openSettings: () -> Void
    let open: (MonitorResult) -> (() -> Void)?
    var body: some View {
        VStack(spacing: 0) {
            TopBar(title: "beacon", key: "cfg", action: openSettings)
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    if let snapshot = model.snapshot {
                        if snapshot.rows.isEmpty {
                            StateCard(phrase: "dark coast", plain: "no monitors", key: ("change config", openSettings))
                        } else {
                            Summary(snapshot: snapshot)
                            ForEach(groups(snapshot.rows), id: \.0) {
                                GroupCard(name: $0.0, rows: $0.1, open: open)
                            }
                        }
                    } else {
                        StateCard(glyph: "↻", phrase: "lighting up",
                                  plain: model.checking.map { "checking \($0) monitors" } ?? "loading config")
                    }
                }
                .padding(.horizontal, 12).padding(.vertical, 12)
            }
            .refreshable { await model.refresh() }
        }
        .background(T.bg)
    }

    /// Groups in the config's order.
    func groups(_ rows: [MonitorResult]) -> [(String, [MonitorResult])] {
        rows.map(\.group).reduce(into: [String]()) { if !$0.contains($1) { $0.append($1) } }
            .map { name in (name, rows.filter { $0.group == name }) }
    }
}

struct Summary: View {
    let snapshot: StatusSnapshot
    var body: some View {
        let down = snapshot.down.count
        VStack(alignment: .leading, spacing: 4) {
            if snapshot.offline {
                HStack(spacing: 8) {
                    Dot(color: T.amber)
                    Text("no network").foregroundStyle(T.amber)
                    Text("· \(snapshot.rows.count) unknown").font(T.font(11)).foregroundStyle(T.mute)
                    checked
                }
            } else {
                HStack(alignment: .firstTextBaseline, spacing: 14) {
                    Text("\(down) down").foregroundStyle(down > 0 ? T.red : T.mute)
                    Text("\(snapshot.upCount) up").foregroundStyle(T.green)
                    checked
                }
                .font(T.font(13, .bold))
            }
            if let failure = snapshot.configFailure {
                HStack(spacing: 8) {
                    Dot(color: T.amber)
                    Text("config from \(T.stamp(snapshot.configAt))").foregroundStyle(T.amber)
                    Text("· \(failure)").font(T.font(11)).foregroundStyle(T.mute).lineLimit(1)
                }
            }
        }
        .font(T.font())
        .padding(.horizontal, 4)
    }

    var checked: some View { Text("checked \(T.stamp(snapshot.at))").font(T.font(11)).foregroundStyle(T.mute) }
}

struct Field: View {
    let label: String
    @Binding var value: String
    var placeholder = ""
    var dim = false
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label).font(T.font(11)).foregroundStyle(T.mute)
            TextField("", text: $value, prompt: Text(placeholder).foregroundStyle(T.mute), axis: .vertical)
                .lineLimit(1...2)
                .keyboardType(.URL).textInputAutocapitalization(.never).autocorrectionDisabled()
                .foregroundStyle(dim ? T.mute : T.fg)
                .disabled(dim)
                .frame(maxWidth: .infinity, minHeight: T.hit, alignment: .leading)
                .padding(.horizontal, 10).padding(.vertical, 6)
                .background(T.card)
                .overlay(RoundedRectangle(cornerRadius: T.radius).stroke(T.edge, lineWidth: 1))
                .clipShape(RoundedRectangle(cornerRadius: T.radius))
        }
    }
}

/// The config-source controls: one https URL and [connect]. The start page and cfg use the same block (q1895).
struct SourceForm: View {
    let model: Model
    var current = false
    var done: () -> Void = {}
    @State private var url = "https://jirathips-mac-mini.tail8c3301.ts.net/config"
    @State private var error: String?
    @State private var connecting = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Field(label: "config url", value: $url, placeholder: "https://…/config", dim: connecting)
            HStack(spacing: 8) {
                Key(title: "connect", enabled: !url.isEmpty && !connecting) { Task { await connect() } }
                if connecting {
                    Text("↻ connecting · reaching the Mini").font(T.font(11)).foregroundStyle(T.mute)
                        .lineLimit(1).truncationMode(.middle)
                }
            }
            if let error {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Dot(color: T.red).alignmentGuide(.firstTextBaseline) { $0[.bottom] }
                    Text("refused").foregroundStyle(T.red)
                    Text(error).foregroundStyle(T.mute).lineLimit(2)
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(T.card)
                .overlay(alignment: .leading) { T.red.frame(width: 3) }
                .clipShape(RoundedRectangle(cornerRadius: T.radius))
            }
            if current, let source = model.source { sourceCard(source) }
            Spacer()
        }
        .font(T.font())
        .padding(16)
        .onAppear { if let source = model.source { url = source.absoluteString } }
    }

    func sourceCard(_ source: URL) -> some View {
        let failure = model.snapshot?.configFailure
        return Card(rule: T.accent) {
            row("source") { Text(source.host() ?? "--").foregroundStyle(T.fg).lineLimit(1).truncationMode(.middle) }
            T.line.frame(height: 1).padding(.leading, 12)
            row("fetched") { Text(model.snapshot.map { T.stamp($0.configAt) } ?? "--").foregroundStyle(T.fg) }
            T.line.frame(height: 1).padding(.leading, 12)
            row("last error") {
                Text(failure ?? "none").foregroundStyle(failure == nil ? T.mute : T.red)
                    .lineLimit(2).multilineTextAlignment(.trailing)
            }
            Spacer().frame(height: 2)
        }
    }

    func row(_ label: String, @ViewBuilder value: () -> some View) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text(label).foregroundStyle(T.mute).fixedSize()
            Spacer(minLength: 0)
            value().font(T.font(11))
        }
        .padding(.horizontal, 12).padding(.vertical, 8)
    }

    func connect() async {
        guard let u = URL(string: url.trimmingCharacters(in: .whitespacesAndNewlines)), u.scheme == "https" else {
            return error = "config url must be https"
        }
        (connecting, error) = (true, nil)
        defer { connecting = false }
        do {
            model.use(u, loaded: try await model.store.load(u))
            done()
        } catch {
            self.error = describe(error)
        }
    }
}
