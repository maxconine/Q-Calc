import AppKit
import SwiftUI
import UniformTypeIdentifiers

// The bar's clip and file commands (native only; the core's parse_share_command): `send file to <name>` picks a
// file and offers it, `paste to <name>` and `send clipboard to <name>` share what's copied. A file dropped on the
// bar goes to whoever the line names (`message max…`, `send file to max`), or waits there for a name; one dropped
// on a thread goes to that person.

/// A share command as the core reads it.
struct ShareCmd: Equatable {
    let what: String
    let to: String
    let name: String
}

extension BarModel {
    func shareCommand(_ typed: String) -> ShareCmd? {
        guard typed.range(of: #"^\s*(send|paste|share)\s"#, options: [.regularExpression, .caseInsensitive]) != nil,
              let o = logic.call("lib/messaging", "parseShareCommand", [typed]) as? [String: Any] else { return nil }
        return ShareCmd(what: o["what"] as? String ?? "", to: o["to"] as? String ?? "", name: o["name"] as? String ?? "")
    }

    /// the hint in the answer slot, and Tab's rest of the name
    func shareLine(_ typed: String) {
        decorations = Decorations()
        let messaging = (try? JSONSerialization.jsonObject(with: Data(Messages.shared.webJSON.utf8))) ?? [:]
        let dropped = MessageFiles.shared.pendingDrop.map { MessageFiles.safeName($0.lastPathComponent) } ?? ""
        commandMessage = logic.string("lib/messaging", "shareHint", [typed, messaging, true, NSNull(), dropped])
        let contacts = logic.call("lib/messaging", "messageContacts", [messaging]) as? [String] ?? []
        let rest = logic.string("lib/messaging", "shareCompletion", [typed, contacts])
        completion = rest.flatMap { $0.isEmpty ? nil : $0 }
    }

    func runShare(_ cmd: ShareCmd) {
        let m = Messages.shared
        if m.name.isEmpty {
            onSettings?()
            return
        }
        guard !cmd.name.isEmpty else { return }
        let name = cmd.name
        let say: (String) -> Void = { [weak self] note in self?.hint = note }
        if cmd.what == "clip" {
            clearLine()
            hint = "sharing with \(name)…"
            MessageFiles.shared.sendClipboard(to: name, done: say)
            return
        }
        if let url = MessageFiles.shared.pendingDrop {
            MessageFiles.shared.pendingDrop = nil
            clearLine()
            MessageFiles.shared.sendFile(url, to: name, done: say)
            return
        }
        clearLine()
        pickFile { url in
            guard let url else { return }
            MessageFiles.shared.sendFile(url, to: name, done: say)
        }
    }

    /// the open panel, over the bar
    func pickFile(_ done: @escaping (URL?) -> Void) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.resolvesAliases = true
        panel.prompt = "Send"
        panel.message = "Pick a file to send (up to 1 GB). It’s encrypted before it leaves this Mac."
        NSApp.activate(ignoringOtherApps: true)
        panel.begin { response in
            done(response == .OK ? panel.url : nil)
        }
    }

    /// files dropped on the bar (to nil: whoever the line names) or on a thread (to them); true when taken
    @discardableResult
    func dropFiles(_ urls: [URL], to who: String?) -> Bool {
        guard let url = urls.first(where: \.isFileURL) else { return false }
        let say: (String) -> Void = { [weak self] note in
            guard let self else { return }
            if let who, self.messagesPanel == .thread(who) { self.threadReply = .failed(note) } else { self.hint = note }
        }
        if let who {
            MessageFiles.shared.sendFile(url, to: who, done: say)
            return true
        }
        let named: String = {
            if let share = shareCommand(text), !share.name.isEmpty { return share.name }
            if case let .message(_, name, _)? = BarCommand.of(text, sheets: sheets) { return name }
            return ""
        }()
        if !named.isEmpty, !Messages.shared.name.isEmpty {
            clearLine()
            MessageFiles.shared.sendFile(url, to: named, done: say)
            return true
        }
        // no name yet: the line asks for one, and ↵ sends it
        MessageFiles.shared.pendingDrop = url
        text = "send file to "
        focusField?()
        return true
    }

    /// file urls from a SwiftUI drop
    func dropProviders(_ providers: [NSItemProvider], to who: String?) -> Bool {
        guard let p = providers.first(where: { $0.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) }) else { return false }
        p.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { [weak self] item, _ in
            let url = (item as? Data).flatMap { URL(dataRepresentation: $0, relativeTo: nil) } ?? item as? URL
            DispatchQueue.main.async {
                if let url { self?.dropFiles([url], to: who) }
            }
        }
        return true
    }
}
