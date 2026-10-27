import AppKit
import SwiftUI
import UniformTypeIdentifiers

// The message threads under the bar (native only; the web app has no thread view). `messages` lists the
// conversations in MessageHistory, latest first, picked with ↑↓ and ↵; `messages <name>`, `messages with <name>`
// and `open messages with <name>` open one (the core's parse_messages_command). A thread shows the newest lines,
// scrolls back, and answers through the same send as `message` and the cards, with the same statuses. Esc or ⌘W
// closes it; a new message from that person lands in it rather than as a card. Drawn like the cards: --bg-tape,
// a hairline on top, --gutter sides, 13 pt lines, 12 pt muted captions, 36 pt list rows with 6 pt corners.

/// What's open under the bar.
enum MessagesPanel: Equatable {
    case list(selected: Int)
    case thread(String)
}

/// A conversation command as the core reads it.
enum ThreadCmd: Equatable {
    case list
    case open(to: String, name: String)
}

extension BarModel {
    /// `messages…` read by the core, or nil when the line isn't one
    func threadCommand(_ typed: String) -> ThreadCmd? {
        guard typed.range(of: #"^\s*(open\s+)?messages"#, options: [.regularExpression, .caseInsensitive]) != nil,
              let o = logic.call("lib/messaging", "parseMessagesCommand", [typed]) as? [String: Any] else { return nil }
        switch o["kind"] as? String {
        case "list": return .list
        case "open": return .open(to: o["to"] as? String ?? "", name: o["name"] as? String ?? "")
        default: return nil
        }
    }

    /// who Tab finishes: the history's names, then who you've sent to, then the accepted
    var threadContacts: [String] {
        let messaging = (try? JSONSerialization.jsonObject(with: Data(Messages.shared.webJSON.utf8))) ?? [:]
        return logic.call("lib/messaging", "threadContacts", [MessageHistory.shared.names, messaging]) as? [String] ?? []
    }

    /// the hint in the answer slot, and Tab's rest of the name
    func messagesLine(_ typed: String, _ cmd: ThreadCmd) {
        decorations = Decorations()
        commandMessage = logic.string("lib/messaging", "messagesHint", [typed])
        let rest = logic.string("lib/messaging", "messagesCompletion", [typed, threadContacts])
        completion = rest.flatMap { $0.isEmpty ? nil : $0 }
    }

    func runThreadCommand(_ cmd: ThreadCmd) {
        switch cmd {
        case .list:
            clearLine()
            messagesPanel = .list(selected: 0)
        case let .open(_, name):
            guard !name.isEmpty else { return }
            clearLine()
            openThread(name)
        }
    }

    func openThread(_ who: String) {
        let name = Messages.clean(who)
        guard !name.isEmpty else { return }
        if messagesPanel != .thread(name) {
            threadDraft = ""
            threadReply = .idle
        }
        messagesPanel = .thread(name)
        // their cards are in the thread now
        cards.removeAll { $0.from == name && !$0.ask }
        if let id = replyFocused, !cards.contains(where: { $0.id == id }) { replyFocused = nil }
    }

    func threadOpen(with who: String) -> Bool { messagesPanel == .thread(who) }

    /// closes the list or the thread; false when neither was open
    @discardableResult
    func closeMessages(refocus: Bool = true) -> Bool {
        guard messagesPanel != nil else { return false }
        messagesPanel = nil
        threadDraft = ""
        threadReply = .idle
        if refocus { focusField?() }
        return true
    }

    /// ↑↓ in the conversations list; false when it isn't up
    func listMove(up: Bool) -> Bool {
        guard case let .list(i) = messagesPanel else { return false }
        let count = MessageHistory.shared.conversations.count
        guard count > 0 else { return true }
        messagesPanel = .list(selected: up ? max(0, i - 1) : min(count - 1, i + 1))
        return true
    }

    /// ↵ in the conversations list opens the picked one; false when it isn't up
    func listEnter() -> Bool {
        guard case let .list(i) = messagesPanel else { return false }
        let list = MessageHistory.shared.conversations
        if list.indices.contains(i) { openThread(list[i].name) }
        return true
    }

    /// the thread's reply line, through Messages.send as `message` and the cards go
    func sendThreadReply() {
        guard case let .thread(name) = messagesPanel else { return }
        let said = String(threadDraft.trimmingCharacters(in: .whitespacesAndNewlines).prefix(Messages.textMax))
        guard !said.isEmpty, threadReply != .sending else { return }
        let m = Messages.shared
        if m.name.isEmpty {
            threadReply = .failed("set your name in settings to send messages")
            return
        }
        threadReply = .sending
        m.send(to: name, text: said) { [weak self] status in
            guard let self, self.messagesPanel == .thread(name) else { return }
            if status == "delivered" {
                m.noteSent(name)
                self.threadDraft = ""
                self.threadReply = .sent
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { [weak self] in
                    if self?.threadReply == .sent { self?.threadReply = .idle }
                }
            } else {
                self.threadReply = .failed(Messages.sendStatusText(status, to: name))
            }
        }
    }
}

// MARK: - the views

/// The list or the thread, under the bar's other panels.
struct MessagesPanelView: View {
    @ObservedObject var model: BarModel
    @ObservedObject private var history = MessageHistory.shared

    var body: some View {
        Group {
            switch model.messagesPanel {
            case let .list(selected)?: ConversationList(model: model, history: history, selected: selected)
            case let .thread(name)?: ThreadView(model: model, name: name, lines: history.thread(name))
            case nil: EmptyView()
            }
        }
        .font(.system(size: Theme.body))
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.qBgTape)
        .overlay(alignment: .top) { Rectangle().fill(Color.qLine).frame(height: 1) }
    }
}

/// a card's header: the title, a muted note, and ×
private struct PanelHeader<Title: View>: View {
    @ViewBuilder var title: Title
    let note: String
    let close: () -> Void
    @State private var hover = false

    var body: some View {
        HStack(spacing: 8) {
            title
            Text(note).font(.system(size: Theme.small)).foregroundStyle(Color.qMuted).lineLimit(1)
            Spacer(minLength: 0)
            Button(action: close) {
                Text("×")
                    .font(.system(size: Theme.small))
                    .foregroundStyle(hover ? Color.qAnswer : Color.qMuted)
                    .padding(.horizontal, 8)
                    .frame(height: 22)
                    .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(hover ? Color.qFillSelected : .clear))
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .onHover { hover = $0 }
            .help("Close (esc)")
            .accessibilityLabel("Close")
        }
        .frame(minHeight: 22)
    }
}

private struct HeightKey: PreferenceKey {
    static var defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

/// `messages`: each conversation's name, last line and time, latest first; ↑↓ and ↵, or a click
private struct ConversationList: View {
    @ObservedObject var model: BarModel
    @ObservedObject var history: MessageHistory
    let selected: Int

    var body: some View {
        let list = history.conversations
        VStack(alignment: .leading, spacing: 4) {
            PanelHeader(title: { Text("messages").fontWeight(.semibold).foregroundStyle(Color.qInk) },
                        note: list.isEmpty ? "" : "↑↓ pick · ↵ open",
                        close: { model.closeMessages() })
                .padding(.horizontal, Theme.gutter)
            if list.isEmpty {
                Text(MessageThreadText.empty("", model.logic))
                    .foregroundStyle(Color.qMuted)
                    .padding(.horizontal, Theme.gutter)
                    .padding(.bottom, 4)
            } else {
                ScrollViewReader { proxy in
                    ScrollView(.vertical, showsIndicators: list.count > Theme.tapeVisible) {
                        LazyVStack(spacing: 0) {
                            ForEach(Array(list.enumerated()), id: \.element.name) { i, c in
                                ConversationRow(name: c.name, last: c.last, selected: i == selected) {
                                    model.openThread(c.name)
                                }
                                .id(i)
                            }
                        }
                    }
                    .frame(height: CGFloat(min(list.count, Theme.tapeVisible)) * Theme.tapeRow)
                    .onChange(of: selected) { proxy.scrollTo($0) }
                }
                .padding(.horizontal, 8)
            }
        }
        .padding(.top, 8)
        .padding(.bottom, 6)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Conversations")
    }
}

private struct ConversationRow: View {
    let name: String
    let last: ThreadMessage
    let selected: Bool
    let open: () -> Void
    @State private var hover = false

    var body: some View {
        Button(action: open) {
            HStack(spacing: 12) {
                Text(name).fontWeight(.semibold).foregroundStyle(Color.qInk).lineLimit(1)
                Text((last.mine ? "you: " : "") + last.text)
                    .foregroundStyle(Color.qMuted)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Text(ThreadTime.label(last.at))
                    .font(.system(size: Theme.small))
                    .monospacedDigit()
                    .foregroundStyle(Color.qMuted)
                    .lineLimit(1)
            }
            .padding(.horizontal, 8)
            .frame(height: Theme.tapeRow)
            .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(selected ? Color.qFillSelected : hover ? Color.qFillHover : .clear))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
        .help("Open messages with \(name)")
        .accessibilityLabel("\(name): \(last.mine ? "you: " : "")\(last.text), \(ThreadTime.label(last.at))")
    }
}

/// one conversation: the newest lines at the bottom, older ones above, and a reply line
private struct ThreadView: View {
    @ObservedObject var model: BarModel
    let name: String
    let lines: [ThreadMessage]
    @FocusState private var focused: Bool
    @State private var contentHeight: CGFloat = 0
    private static let maxHeight: CGFloat = 260

    private static let control = Color(nsColor: NSColor(name: nil) { a in
        a.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? NSColor(srgbRed: 0x58 / 255, green: 0x58 / 255, blue: 0x5c / 255, alpha: 1) : .white
    })

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            PanelHeader(title: { Text(name).fontWeight(.semibold).foregroundStyle(Color.qInk).lineLimit(1) },
                        note: lines.isEmpty ? "" : "\(lines.count) message\(lines.count == 1 ? "" : "s")",
                        close: { model.closeMessages() })
            if lines.isEmpty {
                Text(MessageThreadText.empty(name, model.logic))
                    .foregroundStyle(Color.qMuted)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityLabel(MessageThreadText.empty(name, model.logic))
            } else {
                ScrollViewReader { proxy in
                    ScrollView(.vertical, showsIndicators: contentHeight > Self.maxHeight) {
                        VStack(alignment: .leading, spacing: 0) {
                            ForEach(Array(lines.enumerated()), id: \.element.id) { i, line in
                                ThreadLine(name: name, line: line, headed: Self.headed(lines, i))
                                    .id(line.id)
                            }
                        }
                        .background(GeometryReader { Color.clear.preference(key: HeightKey.self, value: $0.size.height) })
                    }
                    .frame(height: min(max(contentHeight, 1), Self.maxHeight))
                    .onPreferenceChange(HeightKey.self) { h in
                        contentHeight = h
                        if let last = lines.last { proxy.scrollTo(last.id, anchor: .bottom) }
                    }
                    .onAppear { if let last = lines.last { proxy.scrollTo(last.id, anchor: .bottom) } }
                    .onChange(of: lines.last?.id) { id in
                        if let id { withAnimation(.easeOut(duration: 0.15)) { proxy.scrollTo(id, anchor: .bottom) } }
                    }
                }
                .accessibilityElement(children: .contain)
                .accessibilityLabel("Messages with \(name)")
            }
            replyLine.padding(.top, 4)
        }
        .padding(.top, 8)
        .padding(.horizontal, Theme.gutter)
        .padding(.bottom, 12)
        .contentShape(Rectangle())
        // a file dropped on a thread goes to them (ShareCommands.swift)
        .onDrop(of: [.fileURL], isTargeted: nil) { model.dropProviders($0, to: name) }
        .onAppear { DispatchQueue.main.async { focused = true } }
        .onChange(of: name) { _ in DispatchQueue.main.async { focused = true } }
    }

    /// a line says who and when when the sender changes, or 5 minutes have passed
    private static func headed(_ lines: [ThreadMessage], _ i: Int) -> Bool {
        guard i > 0 else { return true }
        return lines[i - 1].dir != lines[i].dir || lines[i].at - lines[i - 1].at > 5 * 60 * 1000
    }

    private var replyLine: some View {
        HStack(spacing: 8) {
            TextField("", text: $model.threadDraft, prompt: Text("Message \(name)").foregroundColor(.qMuted))
                .textFieldStyle(.plain)
                .font(.system(size: Theme.small))
                .foregroundStyle(Color.qInk)
                .padding(.vertical, 4)
                .padding(.horizontal, 8)
                .background(RoundedRectangle(cornerRadius: Theme.radiusS).fill(Self.control)
                    .shadow(color: .black.opacity(0.1), radius: 1, y: 1))
                .overlay(RoundedRectangle(cornerRadius: Theme.radiusS)
                    .strokeBorder(focused ? Color.qAccent : Color.black.opacity(0.12), lineWidth: focused ? 2 : 0.5))
                .noWritingTools()
                .tint(Color.qInk)
                .focused($focused)
                .onSubmit { model.sendThreadReply() }
                .onChange(of: model.threadDraft) { _ in
                    if case .failed = model.threadReply { model.threadReply = .idle }
                }
                .accessibilityLabel("Message \(name)")
            switch model.threadReply {
            case .sending: Text("sending…").font(.system(size: Theme.small)).foregroundStyle(Color.qMuted)
            case .sent: Text("sent").font(.system(size: Theme.small)).foregroundStyle(Color.qAccent)
            case let .failed(note): Text(note).font(.system(size: Theme.small)).foregroundStyle(Color.qMuted).lineLimit(1)
            case .idle: EmptyView()
            }
        }
    }
}

private struct ThreadLine: View {
    let name: String
    let line: ThreadMessage
    let headed: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            if headed {
                HStack(spacing: 6) {
                    Text(line.mine ? "you" : name)
                        .fontWeight(.semibold)
                        .foregroundStyle(line.mine ? Color.qMuted : Color.qInk)
                    Text(ThreadTime.full(line.at)).font(.system(size: Theme.small)).foregroundStyle(Color.qMuted)
                }
                .padding(.top, 6)
            }
            if let clip = line.clip {
                ClipBlock(clip: clip)
            } else if let file = line.file {
                FileBlock(file: file, mine: line.mine, who: name)
            } else {
                Text(line.text)
                    .foregroundStyle(Color.qAnswer)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(.bottom, 2)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(line.mine ? "you" : name), \(ThreadTime.full(line.at)): \(line.text)")
    }
}

/// the core's words for an empty thread or list
enum MessageThreadText {
    static func empty(_ name: String, _ logic: AppLogic) -> String {
        logic.string("lib/messaging", "threadEmptyText", [name])
            ?? (name.isEmpty ? "no messages yet" : "no messages with \(name) yet")
    }
}

// MARK: - settings

/// Messages settings' history rows: what's kept, and a button that clears it all (native only).
struct MessageHistorySection: View {
    @ObservedObject private var history = MessageHistory.shared
    @State private var confirming = false

    var body: some View {
        Section("History") {
            QLabeled {
                Button("Clear Message History…") { confirming = true }
                    .disabled(history.isEmpty)
            } label: {
                Text("Message history")
                Text(summary)
            }
            .confirmationDialog("Clear message history?", isPresented: $confirming) {
                Button("Clear History", role: .destructive) { history.clearAll() }
            } message: {
                Text("Every conversation on this Mac is deleted. Messages already sent aren’t unsent.")
            }
        }
    }

    private var summary: String {
        let n = history.conversations.count
        let kept = "The last \(MessageHistory.cap) messages with each person stay on this Mac. Removing a name deletes its thread."
        return n == 0 ? "None yet. \(kept)" : "\(n) conversation\(n == 1 ? "" : "s"). \(kept)"
    }
}
