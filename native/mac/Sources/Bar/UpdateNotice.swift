import AppKit
import SwiftUI

// A newer Q Calc, said in one muted line under the bar's line (as the privacy line is), so a copy that never
// quits or never sits idle long enough for Sparkle's quiet install doesn't stay old unnoticed.
//
// - Sparkle (Shared/Updates.swift) checks at launch when a day has passed, then daily, and says what it found;
//   nothing found, offline or a failed check says nothing.
// - The version found is kept (qcalc.updateAvailable), so a later launch says it again before Sparkle's next check;
//   once this copy is that version or newer, it's forgotten.
// - "update" asks Sparkle to show it; once downloaded, "install now" installs and relaunches.
// - × lets it go for this run only; the next launch says it again.
final class UpdateNotice: ObservableObject {
    static let shared = UpdateNotice()
    static let latestKey = "qcalc.updateAvailable"

    @Published private(set) var latest: String?
    @Published private(set) var ready = false
    @Published private(set) var dismissed: String?

    static var current: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0.0.0"
    }

    private init() {
        if let kept = UserDefaults.standard.string(forKey: Self.latestKey) {
            if UpdateRules.isNewer(kept, than: Self.current) { latest = kept }
            else { UserDefaults.standard.removeObject(forKey: Self.latestKey) }
        }
    }

    var line: (text: String, action: String)? {
        UpdateRules.line(latest: latest, current: Self.current, ready: ready, dismissed: dismissed)
    }

    /// Sparkle found `version` (downloaded and waiting when `ready`)
    func found(_ version: String, ready: Bool = false) {
        guard UpdateRules.isNewer(version, than: Self.current) else { return upToDate() }
        if version != latest { self.ready = false }
        latest = version
        if ready { self.ready = true }
        UserDefaults.standard.set(version, forKey: Self.latestKey)
    }

    func upToDate() {
        latest = nil
        ready = false
        UserDefaults.standard.removeObject(forKey: Self.latestKey)
    }

    func dismiss() { dismissed = latest }

    func act() { Updates.shared.installOrShow() }
}

struct UpdateNoticeLine: View {
    @ObservedObject private var notice = UpdateNotice.shared
    @State private var hover = false

    var body: some View {
        if let line = notice.line {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Image(systemName: "arrow.down.circle").font(.system(size: Theme.small - 1))
                Text(line.text).lineLimit(1)
                Button { notice.act() } label: { Text(line.action).underline() }
                    .buttonStyle(.plain)
                    .hoverTip(notice.ready ? "Install it and reopen Q Calc" : "Show what's new and install it")
                Spacer(minLength: 0)
                Button { notice.dismiss() } label: {
                    Text("×").foregroundStyle(hover ? Color.qAnswer : Color.qMuted)
                }
                .buttonStyle(.plain)
                .onHover { hover = $0 }
                .hoverTip("Not now")
                .accessibilityLabel("Dismiss")
            }
            .font(.system(size: Theme.small))
            .foregroundStyle(Color.qMuted)
            .padding(.horizontal, Theme.gutter)
            .padding(.bottom, 10)
            .accessibilityElement(children: .contain)
            .accessibilityLabel(line.text)
        }
    }
}
