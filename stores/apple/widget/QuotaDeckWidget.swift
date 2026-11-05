// The widget answers one question: when can I work again.
//
// It reads a snapshot the app wrote into the App Group container and draws it. There is no
// parsing of provider formats here, no computation, no clock arithmetic and no network. Every
// decision that could be wrong — which window leads, which instances are shown, what language
// the labels are in — was made in Rust, where it is tested. A widget has no way to report that
// it got something wrong, so it is given nothing it could get wrong.

import SwiftUI
import WidgetKit

/// The kind the host passes to `WidgetCenter.reloadTimelines`. Must match `WidgetReload.swift`.
private let widgetKind = "QuotaDeckWidget"

/// Above this a level stops being monochrome. The same threshold the menu bar icon uses: a
/// surface that is permanently lit is the reason people remove it.
private let criticalPercent = 85.0

/// `--level-critical`, the one colour this widget is ever allowed to use.
private let criticalColor = Color(red: 1.0, green: 0.37, blue: 0.36)

struct Row: Decodable, Identifiable {
    let instance: String
    let name: String
    let usedPercent: Double?
    let resetsAt: Date?
    let state: String

    var id: String { instance }
}

struct Snapshot: Decodable {
    let capturedAt: Date
    let rows: [Row]
    let emptyMessage: String
    let noResetLabel: String
    let staleLabel: String
}

/// The container, named by the bundle rather than compiled in, because the identifier carries a
/// Team ID and a Team ID is account data.
private func containerURL() -> URL? {
    guard let identifier = Bundle.main.object(forInfoDictionaryKey: "QuotaDeckAppGroup") as? String
    else { return nil }
    return FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: identifier)
}

private func loadSnapshot() -> Snapshot? {
    guard let url = containerURL()?.appendingPathComponent("snapshot.json"),
          let data = try? Data(contentsOf: url)
    else { return nil }
    let decoder = JSONDecoder()
    decoder.dateDecodingStrategy = .iso8601
    return try? decoder.decode(Snapshot.self, from: data)
}

struct Entry: TimelineEntry {
    let date: Date
    let snapshot: Snapshot?
}

struct Provider: TimelineProvider {
    func placeholder(in context: Context) -> Entry {
        Entry(date: Date(), snapshot: nil)
    }

    func getSnapshot(in context: Context, completion: @escaping (Entry) -> Void) {
        completion(Entry(date: Date(), snapshot: loadSnapshot()))
    }

    /// One entry, reloaded at the earliest reset or in half an hour, whichever comes first.
    ///
    /// The app asks for a reload whenever a reset instant moves, so this is a backstop rather
    /// than the primary path: it exists so a widget whose app has not run for a while still
    /// picks the file back up.
    func getTimeline(in context: Context, completion: @escaping (Timeline<Entry>) -> Void) {
        let now = Date()
        let snapshot = loadSnapshot()
        let nextReset = snapshot?.rows.compactMap(\.resetsAt).filter { $0 > now }.min()
        let backstop = now.addingTimeInterval(30 * 60)
        let reload = min(nextReset ?? backstop, backstop)
        let timeline = Timeline(entries: [Entry(date: now, snapshot: snapshot)], policy: .after(reload))
        completion(timeline)
    }
}

private func levelColor(_ percent: Double?) -> Color {
    guard let percent, percent >= criticalPercent else { return .primary }
    return criticalColor
}

struct RowView: View {
    let row: Row
    let noResetLabel: String

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            HStack(spacing: 6) {
                Text(row.name)
                    .font(.caption)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Spacer(minLength: 4)
                if let percent = row.usedPercent {
                    Text("\(Int(percent.rounded()))%")
                        .font(.caption)
                        .monospacedDigit()
                        .foregroundStyle(levelColor(percent))
                }
            }
            // The countdown is the whole reason this widget exists, so it is the largest thing
            // on the row. `.timer` ticks in the system, with no process of ours awake.
            if let resetsAt = row.resetsAt, resetsAt > Date() {
                Text(resetsAt, style: .timer)
                    .font(.title3)
                    .monospacedDigit()
                    .foregroundStyle(levelColor(row.usedPercent))
            } else {
                Text(noResetLabel)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

struct QuotaDeckWidgetView: View {
    @Environment(\.widgetFamily) private var family
    let entry: Entry

    private var rows: [Row] {
        guard let snapshot = entry.snapshot else { return [] }
        let limit = family == .systemSmall ? 1 : 3
        return Array(
            snapshot.rows
                .sorted { ($0.usedPercent ?? 0) > ($1.usedPercent ?? 0) }
                .prefix(limit)
        )
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if rows.isEmpty {
                // Directive, not apologetic: blueprint 7.6.
                Text(entry.snapshot?.emptyMessage ?? "")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(rows) { row in
                    RowView(row: row, noResetLabel: entry.snapshot?.noResetLabel ?? "")
                }
                if let snapshot = entry.snapshot,
                   snapshot.rows.contains(where: { $0.state == "stale" }) {
                    Text("◷ \(snapshot.staleLabel)")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .widgetURL(URL(string: "quotadeck://open"))
    }
}

@main
struct QuotaDeckWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: widgetKind, provider: Provider()) { entry in
            QuotaDeckWidgetView(entry: entry)
        }
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}
