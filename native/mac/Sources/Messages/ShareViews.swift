import AppKit
import SwiftUI

// How a clip and a file look in a card and in a thread (native only; MessageFiles.swift does the work). A clip is
// its text in a tape-coloured box with Copy (⌘C while it has the focus). A file is its name and size, a line on
// where it's got to, a thin bar while it moves, and one or two buttons: Save for an accepted name's (never for a
// request's), Show in Finder once it's saved, Take back for your own on its way. Nothing received is ever opened.

private struct ShareButton: View {
    let title: String
    let action: () -> Void
    @State private var hover = false
    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: Theme.small))
                .foregroundStyle(hover ? Color.qAnswer : Color.qMuted)
                .padding(.horizontal, 8)
                .frame(height: 22)
                .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(hover ? Color.qFillSelected : Color.qFillHover))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
    }
}

/// the lock beside a file or a clip: it was end-to-end encrypted, and only the two of you can open it
struct ShareLock: View {
    let who: String
    var body: some View {
        let tip = MessageFiles.shared.logic?.string("lib/messaging", "lockTooltip", [who])
            ?? "end-to-end encrypted · only you and \(who) can open it"
        Image(systemName: "lock.fill")
            .font(.system(size: Theme.small - 3))
            .foregroundStyle(Color.qMuted)
            .hoverTip(tip)
            .accessibilityLabel(tip)
    }
}

/// shared clipboard text, as it was copied
struct ClipBlock: View {
    let clip: String
    var who = ""
    var lines = 8
    @State private var copied = false
    @FocusState private var focused: Bool

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            ShareLock(who: who).padding(.top, 8)
            Text(clip)
                .font(.system(size: Theme.small, design: .monospaced))
                .foregroundStyle(Color.qAnswer)
                .lineLimit(lines)
                .truncationMode(.tail)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.vertical, 6)
                .padding(.horizontal, 8)
                .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(Color.qFillHover))
                .overlay(RoundedRectangle(cornerRadius: Theme.radiusS).strokeBorder(focused ? Color.qAccent : .clear, lineWidth: 2))
            ShareButton(title: copied ? "Copied" : "Copy") { copy() }
                .hoverTip("Copy to your clipboard (⌘C)")
                .accessibilityLabel("Copy")
        }
        .focusable()
        .focused($focused)
        .onCopyCommand {
            flash()
            return [NSItemProvider(object: clip as NSString)]
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Shared clipboard: \(clip)")
    }

    private func copy() {
        MessageFiles.shared.copy(clip)
        flash()
    }

    private func flash() {
        copied = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { copied = false }
    }
}

/// a file in a card or a thread line
struct FileBlock: View {
    let file: ThreadFile
    let mine: Bool
    let who: String
    @ObservedObject private var files = MessageFiles.shared
    @ObservedObject private var messages = Messages.shared

    var body: some View {
        let t = files.transfer(for: file, mine: mine, who: who)
        let note = files.stateText(t)
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Image(systemName: "doc")
                    .font(.system(size: Theme.small))
                    .foregroundStyle(Color.qMuted)
                ShareLock(who: who)
                Text(file.name)
                    .foregroundStyle(Color.qAnswer)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text(files.sizeText(file.size))
                    .font(.system(size: Theme.small))
                    .monospacedDigit()
                    .foregroundStyle(Color.qMuted)
                    .lineLimit(1)
                Spacer(minLength: 0)
                actions(t)
            }
            if !note.isEmpty {
                Text(note)
                    .font(.system(size: Theme.small))
                    .foregroundStyle(t.state == "failed" ? Color.qInk : Color.qMuted)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
            if let pct = t.percent, t.state == "uploading" || t.state == "downloading" {
                ProgressView(value: Double(min(100, max(0, pct))), total: 100)
                    .progressViewStyle(.linear)
                    .controlSize(.small)
                    .tint(Color.qAccent)
            }
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 8)
        .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(Color.qFillHover))
        .accessibilityElement(children: .contain)
        .accessibilityLabel("\(mine ? "Your" : "\(who)’s") file \(file.name), \(files.sizeText(file.size)). \(note)")
    }

    @ViewBuilder private func actions(_ t: MessageFiles.Transfer) -> some View {
        if mine {
            if files.canRetry(file.id) {
                ShareButton(title: "Retry") { files.retry(file.id) }.hoverTip("Send the rest")
            }
            if t.state == "offered" || t.state == "uploading" || files.canRetry(file.id) {
                ShareButton(title: "Take Back") { files.cancel(file.id) }.hoverTip("Stop sending it")
            }
        } else {
            switch t.state {
            // a request's file waits for Allow
            case "received" where messages.accepted.contains(who), "failed" where messages.accepted.contains(who) && t.saved == nil:
                ShareButton(title: "Save") { files.save(file.id) }.hoverTip("Save to Downloads")
            case "accepting", "waiting", "downloading":
                ShareButton(title: "Cancel") { files.decline(file.id) }.hoverTip("Don’t save it")
            case "saved":
                ShareButton(title: "Show in Finder") { files.reveal(file.id) }
            default:
                EmptyView()
            }
        }
    }
}

/// Messages settings' files row (native only): whether an accepted name's files are saved without asking
struct MessageFilesSection: View {
    @AppStorage(MessageFiles.autoAcceptKey) private var limit = 0
    private static let options: [(String, Int)] = [("Off", 0), ("Up to 10 MB", 10_000_000), ("Up to 100 MB", 100_000_000), ("Up to 1 GB", 1_073_741_824)]

    var body: some View {
        Section("Files") {
            QLabeled {
                // the hidden label is still what VoiceOver reads for the pop-up
                Picker("Save files without asking", selection: $limit) {
                    ForEach(Self.options, id: \.1) { Text($0.0).tag($0.1) }
                }
                .labelsHidden()
                .frame(width: 150)
            } label: {
                Text("Save files without asking")
                Text("Only from accepted names. Files go to Downloads and are never opened. Others wait for Save.")
            }
        }
    }
}

/// Messages settings' privacy rows (native only): what's end-to-end encrypted, what the relay sees and keeps, and
/// what the safety code is for, in the core's words
struct MessagePrivacySection: View {
    var body: some View {
        let lines = MessageFiles.shared.logic?.call("lib/messaging", "privacySettingsText", []) as? [String] ?? []
        Section("Privacy") {
            ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
                Text(line)
                    .fixedSize(horizontal: false, vertical: true)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

/// a contact's safety code and key, under their thread's header when ⓘ is on, and always while their key has
/// changed (native only)
struct ThreadKeyInfo: View {
    let name: String
    let showCode: Bool
    @ObservedObject private var keys = ShareKeys.shared

    private func words(_ fn: String, _ args: [Any]) -> String {
        MessageFiles.shared.logic?.string("lib/messaging", fn, args) ?? ""
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if keys.changed[name] != nil {
                HStack(spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(Color.qInk)
                    Text(words("keyChangedText", [name])).fontWeight(.semibold).foregroundStyle(Color.qInk).lineLimit(1)
                    Spacer(minLength: 0)
                    Button("Trust New Key") { keys.trust(name) }
                        .controlSize(.small)
                        .hoverTip("Pin \(name)’s new key and open what they sent")
                }
                Text(words("keyChangedDetail", [name]))
                    .font(.system(size: Theme.small)).foregroundStyle(Color.qMuted)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if showCode || keys.changed[name] != nil {
                let code = keys.safetyCode(with: name)
                Text(words("safetyCodeText", [name, code]))
                    .font(.system(size: Theme.small, design: code.isEmpty ? .default : .monospaced))
                    .foregroundStyle(code.isEmpty ? Color.qMuted : Color.qAnswer)
                    .textSelection(.enabled)
                    .accessibilityLabel(words("safetyCodeText", [name, code]))
                if !code.isEmpty {
                    Text(words("safetyCodeDetail", [name]))
                        .font(.system(size: Theme.small)).foregroundStyle(Color.qMuted)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .onAppear { if showCode { keys.learn(name) } }
        .onChange(of: showCode) { if $0 { keys.learn(name) } }
    }
}

/// the bar's one-time line on what's encrypted, with × to dismiss it for good
struct PrivacyNoticeLine: View {
    @ObservedObject private var notice = PrivacyNotice.shared
    @State private var hover = false

    var body: some View {
        if notice.showing {
            let text = MessageFiles.shared.logic?.string("lib/messaging", "privacyNotice", []) ?? ""
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Image(systemName: "lock.fill").font(.system(size: Theme.small - 3))
                Text(text).lineLimit(2).fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
                Button { notice.dismiss() } label: {
                    Text("×").foregroundStyle(hover ? Color.qAnswer : Color.qMuted)
                }
                .buttonStyle(.plain)
                .onHover { hover = $0 }
                .hoverTip("Got it")
                .accessibilityLabel("Dismiss")
            }
            .font(.system(size: Theme.small))
            .foregroundStyle(Color.qMuted)
            .padding(.horizontal, Theme.gutter)
            .padding(.bottom, 10)
            .accessibilityElement(children: .combine)
        }
    }
}
