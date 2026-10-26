import AppKit
import Foundation
import SwiftUI
import UserNotifications

extension Notification.Name {
    static let qcalcMessagingChanged = Notification.Name("QCalc.messagingChanged")
    static let qcalcInboxChanged = Notification.Name("QCalc.inboxChanged")
}

// messages between q calc users, through the relay's inboxes (relay/src/index.js, Inbox). a name set in settings
// is held on the relay by a key made here and kept in user defaults; with one set, a websocket waits on the
// relay for messages, which hibernates there, so it costs next to nothing while nobody writes. without a name
// nothing is open. an accepted name's message waits in memory for the bar (MessageInbox), which shows it as a
// card under the bar with a reply line, at once while the bar is up and on its next open otherwise; while the bar
// is away it's a notification too (sender as the title), when q calc may post them. someone new is a request with
// allow and block; a blocked name's messages are dropped unseen. the inbox opens whether or not notifications are
// allowed: they're only the alert, and an app that can't get them (an ad hoc signed build may never be asked)
// would otherwise never receive at all. the page sends with the same name and key, handed to it as
// __QCALC_MESSAGING (src/lib/messaging.ts), along with who you've sent to lately, for the bar to finish names
final class Messages: NSObject, ObservableObject, URLSessionWebSocketDelegate, UNUserNotificationCenterDelegate {
    static let shared = Messages()
    static let relayHost = "qcalc-relay.maxconine.workers.dev"
    private static let nameKey = "qcalc.messaging.name"
    private static let keyKey = "qcalc.messaging.key"
    private static let acceptedKey = "qcalc.messaging.accepted"
    private static let blockedKey = "qcalc.messaging.blocked"
    private static let recentKey = "qcalc.messaging.recent"
    static let recentMax = 12
    private static let requestCategory = "qcalc.messageRequest"
    private static let pingSeconds: TimeInterval = 30
    private static let retryMax: TimeInterval = 60
    static let textMax = 500

    enum Link { case off, connecting, ready, taken, offline }
    // notification permission: unknown until asked
    enum Alerts { case unknown, allowed, denied }

    @Published private(set) var name: String
    @Published private(set) var accepted: [String]
    @Published private(set) var blocked: [String]
    // who you've sent to, the latest first
    private(set) var recent: [String]
    @Published private(set) var link: Link = .off
    // names that asked and haven't been answered, for settings to answer too
    @Published private(set) var requests: [String] = []
    // why the last name change didn't take
    @Published private(set) var note = ""
    @Published private(set) var busy = false
    @Published private(set) var alerts: Alerts = .unknown
    // the bar is up (and no game has it): messages go to it, not to a notification
    var barOpen = false {
        didSet { if barOpen, !oldValue { refreshAlerts() } }
    }

    private var key: String
    private lazy var session = URLSession(configuration: .default, delegate: self, delegateQueue: .main)
    private var task: URLSessionWebSocketTask?
    private var pingTimer: Timer?
    private var retry: DispatchWorkItem?
    private var retryWait: TimeInterval = 3
    private var lastHeard = Date()
    // what each asking name said last, in memory only, shown once they're allowed
    private var asking: [String: String] = [:]
    private var inbox = MessageInbox()
    // permission is asked for once a launch; an app that can't be asked isn't asked on every open
    private var askedAlerts = false

    override init() {
        let defaults = UserDefaults.standard
        key = defaults.string(forKey: Self.keyKey) ?? ""
        name = Self.clean(defaults.string(forKey: Self.nameKey) ?? "")
        accepted = (defaults.stringArray(forKey: Self.acceptedKey) ?? []).map(Self.clean).filter { !$0.isEmpty }
        blocked = (defaults.stringArray(forKey: Self.blockedKey) ?? []).map(Self.clean).filter { !$0.isEmpty }
        recent = MessageInbox.recent(defaults.stringArray(forKey: Self.recentKey) ?? [], max: Self.recentMax, clean: Self.clean)
        super.init()
    }

    // the relay's rule: 2–20 lowercase letters, digits, - or _, starting with a letter or digit
    static func clean(_ raw: String) -> String {
        let name = raw.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return name.range(of: "^[a-z0-9][a-z0-9_-]{1,19}$", options: .regularExpression) != nil ? name : ""
    }

    private static func newKey() -> String {
        (0..<32).map { _ in String(format: "%02x", UInt8.random(in: 0...255)) }.joined()
    }

    func start() {
        let center = UNUserNotificationCenter.current()
        center.delegate = self
        let allow = UNNotificationAction(identifier: "allow", title: "Allow", options: [])
        let block = UNNotificationAction(identifier: "block", title: "Block", options: [.destructive])
        center.setNotificationCategories([
            UNNotificationCategory(identifier: Self.requestCategory, actions: [allow, block], intentIdentifiers: [], options: []),
        ])
        NSWorkspace.shared.notificationCenter.addObserver(
            self, selector: #selector(woke), name: NSWorkspace.didWakeNotification, object: nil
        )
        // back from system settings, maybe with notifications turned on or off
        NotificationCenter.default.addObserver(
            self, selector: #selector(refreshAlerts), name: NSApplication.didBecomeActiveNotification, object: nil
        )
        // the inbox doesn't wait on the permission prompt, which may never be answered
        connect()
        if name.isEmpty {
            refreshAlerts()
        } else {
            askedAlerts = true
            needAlerts { _ in }
        }
    }

    // what settings says under the name
    var status: String {
        if !note.isEmpty { return note }
        if name.isEmpty { return "Off. Set a name to send and receive messages." }
        switch link {
        case .ready: return "Receiving as \(name). Type message <name> <text> in the bar to send."
        case .taken: return "Someone else has \(name) now. Pick another name."
        case .offline: return "Receiving as \(name) once the relay can be reached."
        default: return "Connecting as \(name)…"
        }
    }

    // what the bar hasn't shown yet, for __qcalcInbox
    var inboxJSON: String { inbox.json }

    // the bar showed these
    func seen(_ ids: [String]) {
        inbox.seen(ids)
    }

    // what the page reads as __QCALC_MESSAGING
    var webJSON: String {
        let payload: [String: Any] = ["name": name, "key": key, "accepted": accepted, "blocked": blocked, "recent": recent]
        let data = (try? JSONSerialization.data(withJSONObject: payload, options: [])) ?? Data("{}".utf8)
        return String(data: data, encoding: .utf8) ?? "{}"
    }

    // holds the new name on the relay first; the old one is let go only once the new one is this app's
    func setName(_ raw: String) {
        if raw.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            turnOff()
            return
        }
        let next = Self.clean(raw)
        guard !next.isEmpty else {
            note = "A name is 2–20 letters, digits, - or _."
            return
        }
        guard next != name || link == .taken else {
            note = ""
            return
        }
        if key.isEmpty {
            key = Self.newKey()
            UserDefaults.standard.set(key, forKey: Self.keyKey)
        }
        busy = true
        note = ""
        claim(next)
        // notifications say a message came while the bar is away; without them it waits for the bar
        askedAlerts = true
        needAlerts { _ in }
    }

    // the page sent to them
    func noteSent(_ raw: String) {
        let who = Self.clean(raw)
        guard !who.isEmpty else { return }
        recent = Array(([who] + recent.filter { $0 != who }).prefix(Self.recentMax))
        UserDefaults.standard.set(recent, forKey: Self.recentKey)
        changed()
    }

    private func claim(_ next: String) {
        post("/claim", ["name": next, "key": key]) { [weak self] status in
            guard let self else { return }
            self.busy = false
            switch status {
            case "ok":
                let old = self.name
                if !old.isEmpty, old != next { self.post("/release", ["name": old, "key": self.key]) { _ in } }
                self.name = next
                self.saveName()
                self.connect()
            case "taken": self.note = "\(next) is taken. Try another."
            case "too-many": self.note = "Too many tries. Wait a minute."
            default: self.note = "Couldn’t reach the relay. Try again."
            }
        }
    }

    func turnOff() {
        note = ""
        guard !name.isEmpty else { return }
        post("/release", ["name": name, "key": key]) { _ in }
        name = ""
        asking = [:]
        requests = []
        inbox.clear()
        saveName()
        connect()
    }

    func allow(_ who: String) {
        accepted = (accepted.filter { $0 != who } + [who]).sorted()
        blocked.removeAll { $0 == who }
        inbox.answered(who, blocked: false)
        answered(who)
        if let said = asking.removeValue(forKey: who) { deliver(from: who, text: said) }
        requests = asking.keys.sorted()
    }

    func block(_ who: String) {
        blocked = (blocked.filter { $0 != who } + [who]).sorted()
        accepted.removeAll { $0 == who }
        asking.removeValue(forKey: who)
        requests = asking.keys.sorted()
        inbox.answered(who, blocked: true)
        answered(who)
    }

    // off both lists: their next message asks again
    func forget(_ who: String) {
        accepted.removeAll { $0 == who }
        blocked.removeAll { $0 == who }
        saveLists()
    }

    // the page's own changes to the lists
    func setLists(accepted: [String], blocked: [String]) {
        let b = Array(Set(blocked.map(Self.clean).filter { !$0.isEmpty })).sorted()
        self.blocked = b
        self.accepted = Array(Set(accepted.map(Self.clean).filter { !$0.isEmpty && !b.contains($0) })).sorted()
        for who in b { inbox.answered(who, blocked: true) }
        saveLists()
    }

    private func answered(_ who: String) {
        saveLists()
        UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: ["request-\(who)"])
    }

    private func saveName() {
        UserDefaults.standard.set(name, forKey: Self.nameKey)
        changed()
    }

    private func saveLists() {
        UserDefaults.standard.set(accepted, forKey: Self.acceptedKey)
        UserDefaults.standard.set(blocked, forKey: Self.blockedKey)
        changed()
    }

    private func changed() {
        NotificationCenter.default.post(name: .qcalcMessagingChanged, object: nil)
    }

    // json in, the relay's status out; text/plain like the page's, though only a page needs that
    private func post(_ path: String, _ body: [String: String], done: @escaping (String) -> Void) {
        guard let url = URL(string: "https://\(Self.relayHost)\(path)") else { return done("network") }
        var request = URLRequest(url: url, timeoutInterval: 10)
        request.httpMethod = "POST"
        request.setValue("text/plain", forHTTPHeaderField: "Content-Type")
        request.httpBody = try? JSONSerialization.data(withJSONObject: body, options: [])
        URLSession.shared.dataTask(with: request) { data, _, _ in
            let reply = data.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
            let status = reply?["status"] as? String ?? "network"
            DispatchQueue.main.async { done(status) }
        }.resume()
    }

    // the inbox: one socket while a name is set, none otherwise
    private func connect() {
        close()
        guard !name.isEmpty, !key.isEmpty, let url = URL(string: "wss://\(Self.relayHost)/inbox/\(name)") else {
            link = .off
            return
        }
        link = .connecting
        // the key rides as a protocol, as the page's does, so it's never in a url
        let task = session.webSocketTask(with: url, protocols: ["qcalc-inbox", "k\(key)"])
        self.task = task
        lastHeard = Date()
        task.resume()
        receive(task)
        pingTimer = Timer.scheduledTimer(withTimeInterval: Self.pingSeconds, repeats: true) { [weak self] _ in
            self?.ping()
        }
    }

    private func close() {
        retry?.cancel()
        retry = nil
        pingTimer?.invalidate()
        pingTimer = nil
        task?.cancel(with: .normalClosure, reason: nil)
        task = nil
    }

    private func receive(_ task: URLSessionWebSocketTask) {
        task.receive { [weak self] result in
            DispatchQueue.main.async {
                guard let self, task === self.task else { return }
                switch result {
                case .success(let message):
                    self.lastHeard = Date()
                    if case .string(let text) = message { self.heard(text) }
                    if task === self.task { self.receive(task) }
                case .failure:
                    self.dropped()
                }
            }
        }
    }

    // the relay answers "ping" without waking; two missed answers mean the socket is gone even if it hasn't said
    private func ping() {
        guard let task else { return }
        if Date().timeIntervalSince(lastHeard) > Self.pingSeconds * 2.5 {
            dropped()
            return
        }
        task.send(.string("ping")) { _ in }
    }

    private func heard(_ text: String) {
        guard text.first == "!", let data = String(text.dropFirst()).data(using: .utf8),
              let event = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
        switch event["type"] as? String {
        case "inbox":
            link = .ready
            retryWait = 3
        case "closed":
            // another key holds the name now; trying again won't change that
            if event["reason"] as? String == "taken" || event["reason"] as? String == "released" {
                close()
                link = .taken
            }
        case "message":
            if let from = event["from"] as? String, let said = event["text"] as? String { arrived(from: from, text: said) }
        default:
            break
        }
    }

    private func dropped() {
        guard task != nil else { return }
        close()
        link = .offline
        let work = DispatchWorkItem { [weak self] in self?.connect() }
        retry = work
        DispatchQueue.main.asyncAfter(deadline: .now() + retryWait, execute: work)
        retryWait = min(Self.retryMax, retryWait * 2)
    }

    // asleep, the socket died quietly; awake, it's tried at once
    @objc private func woke() {
        guard !name.isEmpty, link != .taken else { return }
        retryWait = 3
        connect()
    }

    private func arrived(from raw: String, text raw2: String) {
        let from = Self.clean(raw)
        let text = MessageInbox.cleanText(raw2, max: Self.textMax)
        guard !from.isEmpty, !text.isEmpty, !blocked.contains(from) else { return }
        if accepted.contains(from) {
            deliver(from: from, text: text)
            return
        }
        // a name already asking isn't asked about again
        let asked = asking[from] != nil
        asking[from] = text
        requests = asking.keys.sorted()
        guard !asked else { return }
        inbox.add(from: from, text: "", ask: true)
        inboxChanged()
        // the bar asks itself while it's up
        guard !barOpen else { return }
        let content = UNMutableNotificationContent()
        content.body = "\(from) wants to message you. Allow?"
        content.categoryIdentifier = Self.requestCategory
        content.userInfo = ["from": from]
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: "request-\(from)", content: content, trigger: nil))
    }

    // kept for the bar whatever happens to the notification, which is only the alert while the bar is away
    private func deliver(from: String, text: String) {
        guard let item = inbox.add(from: from, text: text, ask: false) else { return }
        inboxChanged()
        guard !barOpen else { return }
        let content = UNMutableNotificationContent()
        // the sender in the title, apart from their words, so no text can pass for another name
        content.title = from
        content.body = text
        content.sound = .default
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: item.id, content: content, trigger: nil))
    }

    private func inboxChanged() {
        NotificationCenter.default.post(name: .qcalcInboxChanged, object: nil)
    }

    private static func alerts(_ status: UNAuthorizationStatus) -> Alerts {
        switch status {
        case .notDetermined: return .unknown
        case .denied: return .denied
        default: return .allowed
        }
    }

    // asks the first time; after that it's the answer given, or what system settings says now
    private func needAlerts(_ done: @escaping (Bool) -> Void) {
        let center = UNUserNotificationCenter.current()
        center.getNotificationSettings { settings in
            guard settings.authorizationStatus == .notDetermined else {
                let now = Self.alerts(settings.authorizationStatus)
                DispatchQueue.main.async { [weak self] in
                    self?.alerts = now
                    done(now == .allowed)
                }
                return
            }
            center.requestAuthorization(options: [.alert, .sound]) { granted, _ in
                DispatchQueue.main.async { [weak self] in
                    self?.alerts = granted ? .allowed : .denied
                    done(granted)
                }
            }
        }
    }

    // turned off in system settings, receiving pauses; turned back on, it picks up again
    // with a name set, permission that was reset is asked for again
    @objc func refreshAlerts() {
        UNUserNotificationCenter.current().getNotificationSettings { settings in
            let now = Self.alerts(settings.authorizationStatus)
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                if now == .unknown, !self.name.isEmpty, !self.askedAlerts {
                    self.askedAlerts = true
                    self.needAlerts { _ in }
                    return
                }
                self.alerts = now
            }
        }
    }

    func openNotificationSettings() {
        let id = Bundle.main.bundleIdentifier ?? ""
        let pane = URL(string: "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=\(id)")
        if let pane, NSWorkspace.shared.open(pane) { return }
        if let old = URL(string: "x-apple.systempreferences:com.apple.preference.notifications") { NSWorkspace.shared.open(old) }
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        let from = response.notification.request.content.userInfo["from"] as? String ?? ""
        DispatchQueue.main.async { [self] in
            switch response.actionIdentifier {
            case "allow": allow(from)
            case "block": block(from)
            default: break
            }
            completionHandler()
        }
    }

    // shown even while the bar or settings is up
    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .sound])
    }
}

// the messages tab: your name, then who's asking, who's accepted and who's blocked. the same rows as the
// windows settings page's MessagingSettings
struct MessagesSettingsView: View {
    @ObservedObject var messages: Messages
    @State private var draft = ""

    var body: some View {
        Form {
            Section {
                LabeledContent {
                    HStack {
                        TextField("Your name", text: $draft, prompt: Text("your name"))
                            .labelsHidden()
                            .noWritingTools()
                            .frame(width: 150)
                            .onSubmit { messages.setName(draft) }
                        Button(messages.busy ? "Checking…" : "Save") { messages.setName(draft) }
                            .disabled(messages.busy || (Messages.clean(draft) == messages.name && messages.link != .taken))
                    }
                } label: {
                    Text("Your name")
                    Text(messages.status)
                }
                if messages.alerts == .denied {
                    LabeledContent {
                        Button("Open Settings") { messages.openNotificationSettings() }
                    } label: {
                        Text("Notifications")
                        Text("Off. Messages wait under the bar for the next time you open it.")
                    }
                }
                if !messages.name.isEmpty {
                    LabeledContent {
                        Button("Turn Off") {
                            messages.turnOff()
                            draft = ""
                        }
                    } label: {
                        Text("Messages")
                        Text("Lets your name go and stops receiving")
                    }
                }
            }
            if !messages.requests.isEmpty {
                Section("Asking to message you") {
                    ForEach(messages.requests, id: \.self) { who in
                        LabeledContent(who) {
                            HStack {
                                Button("Allow") { messages.allow(who) }
                                Button("Block") { messages.block(who) }
                            }
                        }
                    }
                }
            }
            names("Accepted", messages.accepted, empty: "Nobody yet. Anyone new’s first message asks you.", action: "Remove")
            names("Blocked", messages.blocked, empty: "Nobody. A blocked name’s messages never show, and they aren’t told.", action: "Unblock")
        }
        .formStyle(.grouped)
        .onAppear {
            draft = messages.name
            messages.refreshAlerts()
        }
    }

    private func names(_ title: String, _ list: [String], empty: String, action: String) -> some View {
        Section(title) {
            if list.isEmpty {
                Text(empty).foregroundStyle(.secondary)
            }
            ForEach(list, id: \.self) { who in
                LabeledContent(who) {
                    Button(action) { messages.forget(who) }
                }
            }
        }
    }
}
