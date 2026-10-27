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

/// shared clipboard text, as it was copied
struct ClipBlock: View {
    let clip: String
    var lines = 8
    @State private var copied = false
    @FocusState private var focused: Bool

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
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
                .help("Copy to your clipboard (⌘C)")
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
            if t.state == "offered" || t.state == "uploading" {
                ShareButton(title: "Take Back") { files.cancel(file.id) }.help("Stop sending it")
            }
        } else {
            switch t.state {
            // a request's file waits for Allow
            case "received" where messages.accepted.contains(who), "failed" where messages.accepted.contains(who) && t.saved == nil:
                ShareButton(title: "Save") { files.save(file.id) }.help("Save to Downloads")
            case "accepting", "waiting", "downloading":
                ShareButton(title: "Cancel") { files.decline(file.id) }.help("Don’t save it")
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
                Picker("", selection: $limit) {
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
