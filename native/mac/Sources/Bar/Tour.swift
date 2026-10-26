import Foundation

// Onboarding and the walkthrough, as QuickCalc.tsx runs them, with the rules from src/lib/onboarding.ts and tour.ts
// (through AppLogic): the open's hint and the examples that rotate in the empty bar, a hint after an answer has sat
// still, a hint after a save, the tutorial's steps with their line under the bar (`n/m`, skip), and the "you can
// also" extras on later opens. State is UserDefaults `qcalc.onboarding`, as before.

/// What the empty bar shows: an example (and its answer) fading in and out.
struct ExampleShown: Equatable {
    var id: Int
    var expr: String
    var answer: String
}

/// The tutorial's line under the bar.
struct TourLineShown: Equatable {
    var text: String
    var at: Int
    var of: Int
}

extension BarModel {
    private var hotkeyTitle: String { (settings.activeHotKey ?? settings.hotKey).title }

    // ---- the stored state

    func onboardingState() -> [String: Any] {
        let o = settings.onboarding
        let raw: [String: Any] = ["opens": o["opens"] ?? 0, "commits": o["commits"] ?? 0, "hints": o["hints"] ?? 0, "done": (o["done"] ?? 0) > 0]
        return logic.call("lib/onboarding", "sanitizeOnboarding", [raw]) as? [String: Any] ?? raw
    }

    private func save(_ o: [String: Any]) {
        func n(_ k: String) -> Int { (o[k] as? NSNumber)?.intValue ?? 0 }
        settings.mergeOnboarding(["opens": n("opens"), "commits": n("commits"), "hints": n("hints"), "done": (o["done"] as? Bool) == true ? 1 : 0])
    }

    private var hints: Int { (onboardingState()["hints"] as? NSNumber)?.intValue ?? 0 }

    // ---- the walkthrough

    func touringNow() -> Bool { replay != nil || logic.bool("lib/tour", "tourRunning", [onboardingState()]) }

    private var tourDone: Int { replay ?? hints }

    /// the step the walkthrough is on, when it's running
    func tourStepNow() -> [String: Any]? {
        guard touringNow() else { return nil }
        return logic.call("lib/tour", "tourStep", [tourDone, hotkeyTitle]) as? [String: Any]
    }

    /// the walkthrough owns the line under the bar until its basics are done
    func tourBasicsLeft() -> Bool { touringNow() && tourStepNow() != nil }

    func markTour(_ bits: Int) {
        guard bits != 0 else { return }
        if hints & bits != bits {
            var o = onboardingState()
            o["hints"] = hints | bits
            save(o)
        }
        if let r = replay { replay = r | bits }
        refreshTour()
    }

    func startTutorial() {
        replay = 0
        rotation = (false, 0)
        hint = nil
        text = ""
        refreshTour()
    }

    func skipTutorial() {
        if replay != nil {
            replay = nil
        } else if let off = logic.call("lib/tour", "tourBit", ["off"]) as? NSNumber {
            // TOUR_OFF
            markTour(off.intValue)
        }
        hint = "skipped · type tutorial any time to see it again"
        refreshTour()
        focusField?()
    }

    /// the line under the bar and the example the step shows, refreshed after anything that moves the tour
    func refreshTour() {
        if let step = tourStepNow(), let stepText = step["text"] as? String {
            let progress = logic.call("lib/tour", "tourProgress", [tourDone, hotkeyTitle]) as? [String: Any]
            let fits = logic.bool("lib/tour", "tourLineFits", [step, text, !live.isBlank])
            tourLine = fits ? TourLineShown(text: stepText, at: (progress?["at"] as? NSNumber)?.intValue ?? 0,
                                            of: (progress?["of"] as? NSNumber)?.intValue ?? 0) : nil
            ghostExample = (step["plain"] as? Bool) == true ? nil : step["expr"] as? String
            ghostPlain = (step["plain"] as? Bool) == true ? step["expr"] as? String : nil
        } else {
            tourLine = nil
            if hintGhost?.hint != hint { hintGhost = nil }
            ghostExample = hintGhost?.plain == true ? nil : hintGhost?.expr
            ghostPlain = hintGhost?.plain == true ? hintGhost?.expr : nil
        }
        refreshExample()
    }

    // ---- each showing (beginShowing, beginTour)

    func beginShowing() {
        var o = onboardingState()
        o = logic.call("lib/onboarding", "recordOpen", [o]) as? [String: Any] ?? o
        o = logic.call("lib/tour", "enrolTour", [o, true]) as? [String: Any] ?? o
        save(o)
        rotation = (!touringNow() && logic.bool("lib/onboarding", "examplesActive", [onboardingState()]), 0)
        if settings.hotKeyFailed {
            hint = "no shortcut works — open Q Calc from the menu bar icon, or change it in the menu"
        } else {
            let facts: [String: Any] = ["expr": "", "native": true, "opens": (o["opens"] as? NSNumber)?.intValue ?? 0]
            if let opened = logic.call("lib/onboarding", "pickHint", [hints, facts, "open"]) as? [String: Any] {
                markHint(opened)
                hint = opened["text"] as? String
            }
        }
        // an open after the first is the shortcut learned; once the basics are done each open brings one extra
        if ((o["opens"] as? NSNumber)?.intValue ?? 0) >= 2 || replay != nil,
           let bit = logic.call("lib/tour", "tourBit", ["hotkey"]) as? NSNumber {
            markTour(bit.intValue)
        }
        if touringNow(), !settings.hotKeyFailed,
           let extra = logic.call("lib/tour", "tourExtra", [tourDone, hotkeyTitle]) as? [String: Any],
           let text = extra["text"] as? String {
            markTour((extra["bit"] as? NSNumber)?.intValue ?? 0)
            hint = text
            hintGhost = (text, extra["expr"] as? String ?? "", (extra["plain"] as? Bool) == true)
        }
        refreshTour()
        startRotationTimer()
    }

    private func markHint(_ h: [String: Any]) {
        guard let bit = (h["bit"] as? NSNumber)?.intValue else { return }
        var o = onboardingState()
        o["hints"] = hints | bit
        save(o)
    }

    // ---- examples in the empty bar (EXAMPLES, rotation every EXAMPLE_MS until the first keystroke)

    private func startRotationTimer() {
        rotationTimer?.invalidate()
        guard rotation.on else { return }
        rotationTimer = Timer.scheduledTimer(withTimeInterval: 2.5, repeats: true) { [weak self] _ in
            guard let self, self.rotation.on else { return }
            self.rotation.tick += 1
            self.refreshExample()
        }
    }

    func dismissRotation() {
        guard rotation.on else { return }
        rotation = (false, 0)
        rotationTimer?.invalidate()
    }

    func refreshExample() {
        guard text.isEmpty, !helpShown else {
            example = nil
            return
        }
        let id: Int
        let expr: String
        var plain = false
        if let g = ghostExample {
            expr = g
            id = -1 - (tourLine?.at ?? 0)
        } else if let p = ghostPlain {
            expr = p
            plain = true
            id = -1 - (tourLine?.at ?? 0)
        } else if rotation.on, let list = logic.call("lib/onboarding", "exampleList", [firstRunHotkey ?? NSNull()]) as? [[String: Any]], !list.isEmpty {
            let item = list[rotation.tick % list.count]
            expr = item["expr"] as? String ?? ""
            plain = (item["plain"] as? Bool) == true
            id = rotation.tick
        } else {
            example = nil
            return
        }
        example = ExampleShown(id: id, expr: expr, answer: "")
        guard !plain else { return }
        // exampleAnswer: evaluated as the settings say, exact ≈ approx when there are two, `x =` for a solve
        let options = self.options, typed = Symbols.pretty(expr, caret: (expr as NSString).length, keepWords: settings.keepWords).text
        engine.submit(.forms, { engine -> String in
            guard let line = engine.evaluate(lines: [typed], options: options).first, !line.display.isEmpty else { return "" }
            var answer = line.display
            if let e = line.exact, e.trimmingCharacters(in: .whitespaces) != line.display.trimmingCharacters(in: .whitespaces) {
                answer = "\(e) ≈ \(line.display)"
            }
            if line.kind == .solve, let v = line.variable { answer = "\(v) = \(answer)" }
            return answer
        }) { [weak self] answer in
            guard let self, self.example?.id == id, self.example?.expr == expr else { return }
            self.example?.answer = answer
        }
    }

    // ---- hints after an answer sits still, and after a save

    func pauseHintLater() {
        pauseTimer?.invalidate()
        guard !text.trimmingCharacters(in: .whitespaces).isEmpty, !live.isBlank, !live.improper, graph == nil, !helpShown else { return }
        let typed = text
        pauseTimer = Timer.scheduledTimer(withTimeInterval: 1.5, repeats: false) { [weak self] _ in
            guard let self, self.text == typed, !self.tourBasicsLeft() else { return }
            if let next = self.logic.call("lib/onboarding", "pickHint", [self.hints, self.hintFacts(typed), "pause"]) as? [String: Any] {
                self.markHint(next)
                self.hint = next["text"] as? String
            }
        }
    }

    func hintFacts(_ expr: String) -> [String: Any] {
        var f: [String: Any] = ["expr": expr, "answer": live.display, "unit": live.unit != nil, "chained": chained,
                                "angleMode": angleRad ? "rad" : "deg", "fractionMode": fractions, "native": true]
        if let v = live.variable { f["variable"] = v }
        return f
    }

    /// after a save: the commit's hint (unless the tour has the line), the count, and which tour steps it used
    func onboardingAfterCommit(expr: String, facts: [String: Any], solve: Bool, graph: Bool) -> String? {
        let next = tourBasicsLeft() ? nil : logic.call("lib/onboarding", "pickHint", [hints, facts, "commit"]) as? [String: Any]
        var o = onboardingState()
        o = logic.call("lib/onboarding", "recordCommit", [o]) as? [String: Any] ?? o
        o["hints"] = ((o["hints"] as? NSNumber)?.intValue ?? 0) | ((next?["bit"] as? NSNumber)?.intValue ?? 0)
        save(o)
        let used: [String: Any] = ["expr": expr, "unit": (facts["unit"] as? Bool) == true, "chained": (facts["chained"] as? Bool) == true,
                                   "solve": solve, "graph": graph]
        if let bits = logic.call("lib/tour", "tourUsed", [used]) as? NSNumber { markTour(bits.intValue) }
        if replay != nil, tourStepNow() == nil, let done = logic.string("lib/tour", "TOUR_DONE_HINT", []) {
            replay = nil
            return done
        }
        return next?["text"] as? String
    }
}
