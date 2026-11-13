import BeaconKit
import SwiftUI

struct Root: View {
    let model: Model
    @State private var settings = false
    @Environment(\.scenePhase) private var phase

    var body: some View {
        if model.source == nil {
            VStack(spacing: 0) {
                TopBar(title: "beacon")
                SourceForm(model: model)
            }
            .background(T.bg)
        } else {
            MonitorList(model: model) { settings = true }
                .task(id: phase) {
                    while phase == .active, !Task.isCancelled {
                        await model.refresh()
                        try? await Task.sleep(for: .seconds(60))
                    }
                }
                .sheet(isPresented: $settings) {
                    VStack(spacing: 0) {
                        TopBar(title: "config", key: "done") { settings = false }
                        SourceForm(model: model, current: true) { settings = false }
                    }
                    .background(T.bg)
                    .presentationBackground(T.bg)
                }
        }
    }
}

/// A lowercase `[key]` in the accent colour, with a 44 pt hit area whatever its text width.
struct Key: View {
    let title: String
    var enabled = true
    let action: () -> Void
    var body: some View {
        Button(action: action) {
            Text("[\(title)]")
                .font(T.font())
                .foregroundStyle(enabled ? T.accent : T.mute)
                .frame(minWidth: T.hit, minHeight: T.hit)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
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
    var body: some View {
        let down = rows.count { $0.state == .down }
        let unknown = rows.allSatisfy { $0.state == .unknown }
        Card(rule: unknown ? T.mute : down > 0 ? T.red : T.green) {
            HStack(spacing: 10) {
                Text("~ \(name.lowercased())").font(T.font(13, .bold)).foregroundStyle(T.fg)
                Group {
                    if unknown { Text("\(rows.count) unknown").foregroundStyle(T.mute) }
                    else if down > 0 { Text("\(down) down").foregroundStyle(T.red) }
                    else { Text("\(rows.count) up").foregroundStyle(T.mute) }
                }
                .font(T.font(11))
            }
            .padding(.horizontal, 12).padding(.top, 11).padding(.bottom, 5)
            ForEach(Array(rows.enumerated()), id: \.offset) { i, row in
                if i > 0 { T.line.frame(height: 1).padding(.leading, 12) }
                MonitorRow(row: row)
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
                            ForEach(groups(snapshot.rows), id: \.0) { GroupCard(name: $0.0, rows: $0.1) }
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
                }
            } else {
                HStack(spacing: 14) {
                    Text("\(down) down").foregroundStyle(down > 0 ? T.red : T.mute)
                    Text("\(snapshot.upCount) up").foregroundStyle(T.green)
                }
                .font(T.font(13, .bold))
            }
            if snapshot.configStale {
                HStack(spacing: 8) {
                    Dot(color: T.amber)
                    Text("config from \(T.stamp(snapshot.configAt))").foregroundStyle(T.amber)
                    Text("· fetch failed").font(T.font(11)).foregroundStyle(T.mute)
                }
            }
        }
        .font(T.font())
        .padding(.horizontal, 4)
    }
}

struct Field: View {
    let label: String
    @Binding var value: String
    var placeholder = ""
    var secure = false
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label).font(T.font(11)).foregroundStyle(T.mute)
            Group {
                if secure { SecureField("", text: $value, prompt: prompt) }
                else { TextField("", text: $value, prompt: prompt) }
            }
            .textInputAutocapitalization(.never).autocorrectionDisabled()
            .foregroundStyle(T.fg)
            .frame(maxWidth: .infinity, minHeight: T.hit, alignment: .leading)
            .padding(.horizontal, 10)
            .background(T.card)
            .overlay(RoundedRectangle(cornerRadius: T.radius).stroke(T.edge, lineWidth: 1))
            .clipShape(RoundedRectangle(cornerRadius: T.radius))
        }
    }

    var prompt: Text { Text(placeholder).foregroundStyle(T.mute.opacity(0.7)) }
}

/// The config-source controls: first run and settings use the same block (q1578).
struct SourceForm: View {
    let model: Model
    var current = false
    var done: () -> Void = {}
    @State private var url = ""
    @State private var headerName = ""
    @State private var headerValue = ""
    @State private var error: String?
    @State private var importing = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            if current { currentCard }
            Field(label: "config url", value: $url, placeholder: "https://…/monitors.json")
            Field(label: "header name (optional)", value: $headerName, placeholder: "Authorization")
            Field(label: "header value (keychain)", value: $headerValue, secure: true)
            HStack(spacing: 4) {
                Key(title: "load", enabled: !url.isEmpty) { Task { await load() } }
                Key(title: "import from files") { importing = true }
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
            Spacer()
        }
        .font(T.font())
        .padding(16)
        .onAppear {
            guard case .url(let u, let name) = model.source else { return }
            (url, headerName, headerValue) = (u.absoluteString, name ?? "", Keychain.header ?? "")
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.json]) { result in
            do {
                let file = try result.get()
                let scoped = file.startAccessingSecurityScopedResource()
                defer { if scoped { file.stopAccessingSecurityScopedResource() } }
                model.use(.local, loaded: try model.store.importLocal(Data(contentsOf: file)))
                done()
            } catch {
                self.error = describe(error)
            }
        }
    }

    var currentCard: some View {
        let header: String = if case .url(_, let name?) = model.source { "\(name) ••••" } else { "none" }
        let rows = [
            ("source", model.source == .local ? "local" : "url"),
            ("fetched", model.snapshot.map { T.stamp($0.configAt) } ?? "--"), ("header", header),
        ]
        return Card(rule: T.accent) {
            ForEach(Array(rows.enumerated()), id: \.offset) { i, kv in
                if i > 0 { T.line.frame(height: 1).padding(.leading, 12) }
                HStack {
                    Text(kv.0).foregroundStyle(T.mute)
                    Spacer()
                    Text(kv.1).foregroundStyle(T.fg).lineLimit(1).truncationMode(.middle)
                }
                .padding(.horizontal, 12).padding(.vertical, 8)
            }
            Spacer().frame(height: 2)
        }
    }

    func load() async {
        let name = headerName.trimmingCharacters(in: .whitespaces)
        guard let u = URL(string: url.trimmingCharacters(in: .whitespaces)), u.scheme == "https" else {
            return error = "config url must be https"
        }
        let source = ConfigSource.url(u, headerName: name.isEmpty ? nil : name)
        do {
            let loaded = try await model.store.load(source, header: headerValue)
            Keychain.setHeader(name.isEmpty ? "" : headerValue)
            model.use(source, loaded: loaded)
            done()
        } catch {
            self.error = describe(error)
        }
    }
}
