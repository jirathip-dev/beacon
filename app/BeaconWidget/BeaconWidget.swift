import BeaconKit
import SwiftUI
import WidgetKit

struct Entry: TimelineEntry {
    let date: Date
    let snapshot: StatusSnapshot?
}

/// No network and no Keychain: only the snapshot the app saved (ADR D8). The app reloads the timeline.
struct Provider: TimelineProvider {
    func placeholder(in context: Context) -> Entry { Entry(date: .now, snapshot: nil) }

    func getSnapshot(in context: Context, completion: @escaping (Entry) -> Void) {
        completion(Entry(date: .now, snapshot: StatusSnapshot.load()))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<Entry>) -> Void) {
        completion(Timeline(entries: [Entry(date: .now, snapshot: StatusSnapshot.load())], policy: .never))
    }
}

struct DownNames: View {
    let names: [String]
    let max: Int
    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            ForEach(Array(names.prefix(max).enumerated()), id: \.offset) { _, name in
                HStack(spacing: 6) {
                    Dot(color: T.red)
                    Text(name).foregroundStyle(T.fg).lineLimit(1).truncationMode(.tail)
                }
            }
            if names.count > max { Text("+\(names.count - max) more").foregroundStyle(T.mute) }
        }
    }
}

struct Counts: View {
    let snapshot: StatusSnapshot?
    var body: some View {
        let down = snapshot?.down.count ?? 0
        VStack(alignment: .leading, spacing: 1) {
            if snapshot == nil {
                Text("--").foregroundStyle(T.mute).font(T.font(15, .bold))
            } else if down == 0 {
                Text("all up").foregroundStyle(T.green).font(T.font(15, .bold))
            } else {
                Text("\(down) down").foregroundStyle(T.red).font(T.font(15, .bold))
            }
            Text("\(snapshot?.upCount ?? 0) up").foregroundStyle(down == 0 ? T.mute : T.green)
        }
    }
}

struct Checked: View {
    let snapshot: StatusSnapshot?
    var body: some View {
        Text("checked \(snapshot.map { T.stamp($0.at) } ?? "--")").foregroundStyle(T.mute).font(T.font(10))
    }
}

struct WidgetView: View {
    let entry: Entry
    @Environment(\.widgetFamily) private var family

    var body: some View {
        let names = entry.snapshot?.down.map(\.name) ?? []
        Group {
            if family == .systemMedium {
                HStack(alignment: .top, spacing: 14) {
                    VStack(alignment: .leading, spacing: 0) {
                        Text("beacon").font(T.font(11, .bold)).foregroundStyle(T.mute).padding(.bottom, 6)
                        Counts(snapshot: entry.snapshot)
                        Spacer()
                        Checked(snapshot: entry.snapshot)
                    }
                    .frame(width: 96, alignment: .leading)
                    DownNames(names: names, max: 5)
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                        .overlay(alignment: .leading) { T.line.frame(width: 1).offset(x: -8) }
                }
            } else {
                VStack(alignment: .leading, spacing: 0) {
                    Counts(snapshot: entry.snapshot)
                    Spacer(minLength: 6)
                    DownNames(names: names, max: 3)
                    Spacer(minLength: 6)
                    Checked(snapshot: entry.snapshot)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            }
        }
        .font(T.font(11))
        .containerBackground(T.bg, for: .widget)
    }
}

@main
struct BeaconWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "Beacon", provider: Provider()) { WidgetView(entry: $0) }
            .configurationDisplayName("Beacon")
            .supportedFamilies([.systemSmall, .systemMedium])
    }
}
