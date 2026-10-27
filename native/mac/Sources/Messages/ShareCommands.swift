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
        let pending = MessageFiles.shared.pendingDrop
        let dropped = pending.first.flatMap { logic.string("lib/messaging", "droppedLabel", [$0.lastPathComponent, pending.count]) } ?? ""
        commandMessage = logic.string("lib/messaging", "shareHint", [typed, messaging, true, NSNull(), dropped])
        let contacts = logic.call("lib/messaging", "messageContacts", [messaging]) as? [String] ?? []
        let rest = logic.string("lib/messaging", "shareCompletion", [typed, contacts])
        completion = rest.flatMap { $0.isEmpty ? nil : $0 }
    }

    func runShare(_ cmd: ShareCmd) {
        let m = Messages.shared
        if m.name.isEmpty {
            (onMessagesSettings ?? onSettings)?()
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
        let dropped = MessageFiles.shared.pendingDrop
        if !dropped.isEmpty {
            MessageFiles.shared.pendingDrop = []
            clearLine()
            dropped.forEach { MessageFiles.shared.sendFile($0, to: name, done: say) }
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

    /// where a drop on the bar goes with the line as it is (the core's dropTarget): `message <name> [text]` or
    /// `send file to <name>` names who; `hint` says so, or why it can't go; nil when the line names nobody
    func dropTarget(count: Int) -> (name: String, text: String, hint: String)? {
        let messaging = (try? JSONSerialization.jsonObject(with: Data(Messages.shared.webJSON.utf8))) ?? [:]
        let ask = { (found: Any) in self.logic.call("lib/messaging", "dropTarget", [self.text, messaging, true, found, count]) as? [String: Any] }
        guard var o = ask(NSNull()) else { return nil }
        // the relay's word on the name, when it's had one, as `message` uses it
        if let name = o["name"] as? String, !name.isEmpty, recipientFound(name) == false, let again = ask(false) { o = again }
        let hint = o["hint"] as? String ?? ""
        guard !hint.isEmpty else { return nil }
        return (o["name"] as? String ?? "", o["text"] as? String ?? "", hint)
    }

    /// while files are dragged over the bar, the hint says where they'll go (or what's wrong with the name)
    func dragHover(_ over: Bool, count: Int = 1) {
        if over, let target = dropTarget(count: max(1, count)) {
            if dragHint == nil { dragHint = hint ?? "" }
            hint = target.hint
        } else if let before = dragHint {
            dragHint = nil
            hint = before.isEmpty ? nil : before
        }
    }

    /// files dropped on the bar (to nil: whoever the line names) or on a thread (to them); true when taken
    @discardableResult
    func dropFiles(_ urls: [URL], to who: String?) -> Bool {
        let say: (String) -> Void = { [weak self] note in
            guard let self else { return }
            if let who, self.messagesPanel == .thread(who) { self.threadReply = .failed(note) } else { self.hint = note }
        }
        // folders don't go (sendFile would only say it couldn't read one): say so when nothing else was dropped
        let isFolder = { (u: URL) in (try? u.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true }
        let all = urls.filter(\.isFileURL)
        let files = all.filter { !isFolder($0) }
        guard !files.isEmpty else {
            if !all.isEmpty { dragHint = nil; say(MessageFiles.shared.errorText("folder", who: who ?? "")) }
            return !all.isEmpty
        }
        if let who {
            files.forEach { MessageFiles.shared.sendFile($0, to: who, done: say) }
            return true
        }
        let target = dropTarget(count: files.count)
        dragHint = nil
        if let target, !target.name.isEmpty {
            // `message max2 here you go` and a drop: the words go as a message, and each file as it would by
            // `send file to max2`
            clearLine()
            if !target.text.isEmpty {
                Messages.shared.send(to: target.name, text: target.text) { status in
                    if status == "delivered" { Messages.shared.noteSent(target.name) }
                    if status != "delivered" { say(Messages.sendStatusText(status, to: target.name)) }
                }
            }
            files.forEach { MessageFiles.shared.sendFile($0, to: target.name, done: say) }
            return true
        }
        if let target {
            // a message or share line whose name can't take it yet: say why, and leave the line as it is
            hint = target.hint
            return true
        }
        // no name yet: the line asks for one, and ↵ sends it
        MessageFiles.shared.pendingDrop = files
        // an earlier drop's word (a folder, a bad name) is stale now that the line asks for a name
        hint = nil
        text = "send file to "
        focusField?()
        return true
    }

    /// file urls from a SwiftUI drop, all of them, in the order they were dragged
    func dropProviders(_ providers: [NSItemProvider], to who: String?) -> Bool {
        let items = providers.filter { $0.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) }
        guard !items.isEmpty else { return false }
        var urls = [URL?](repeating: nil, count: items.count)
        let group = DispatchGroup()
        for (i, p) in items.enumerated() {
            group.enter()
            p.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { item, _ in
                let url = (item as? Data).flatMap { URL(dataRepresentation: $0, relativeTo: nil) } ?? item as? URL
                DispatchQueue.main.async {
                    urls[i] = url
                    group.leave()
                }
            }
        }
        group.notify(queue: .main) { [weak self] in
            let got = urls.compactMap { $0 }
            if !got.isEmpty { self?.dropFiles(got, to: who) }
        }
        return true
    }
}
