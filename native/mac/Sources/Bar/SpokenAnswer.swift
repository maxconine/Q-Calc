import AppKit
import Combine

/// VoiceOver: the live answer read out once typing pauses, as the page's aria-live answer is (native only; before,
/// only a selection's answer was spoken, so hearing the answer meant leaving the line for it). Only while VoiceOver
/// is on, at the normal priority so a key echo isn't cut off; the rule is `TapeRules.liveSpoken`.
final class SpokenAnswer {
    private var bag = Set<AnyCancellable>()
    private var last = ""

    init(_ model: BarModel) {
        model.$live
            .debounce(for: .milliseconds(700), scheduler: RunLoop.main)
            .sink { [weak self, weak model] _ in
                guard let self, let model else { return }
                if model.text.trimmingCharacters(in: .whitespaces).isEmpty { self.last = ""; return }
                guard NSWorkspace.shared.isVoiceOverEnabled, model.selectionAnswer == nil else { return }
                let answer = model.shownAnswer
                guard let line = TapeRules.liveSpoken(answer: answer, text: model.text, last: self.last) else { return }
                self.last = answer.trimmingCharacters(in: .whitespaces)
                BarModel.announce(line)
            }
            .store(in: &bag)
    }
}
