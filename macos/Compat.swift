import SwiftUI

// macOS 12 stand-ins for the settings forms' macOS 13 SwiftUI (LabeledContent, the grouped form style). On 13 and
// later each one is exactly the 13 API, so those Macs look as they always have; on 12 they are the closest
// Monterey has (native/mac/PARITY.md lists what differs there).

/// `LabeledContent { content } label: { title; subtitle… }`: on 12 the title, any further label lines as the
/// grouped form's smaller grey subtitle, and the content trailing.
struct QLabeled<Label: View, Content: View>: View {
    private let content: Content
    private let label: Label

    init(@ViewBuilder content: () -> Content, @ViewBuilder label: () -> Label) {
        self.content = content()
        self.label = label()
    }

    var body: some View {
        if #available(macOS 13, *) {
            LabeledContent { content } label: { label }
        } else {
            HStack(alignment: .center, spacing: 12) {
                _VariadicView.Tree(SubtitleLines()) { label }
                Spacer(minLength: 8)
                content
            }
            .padding(.vertical, 2)
        }
    }
}

extension QLabeled where Label == Text {
    /// `LabeledContent("Title") { content }`
    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.init(content: content, label: { Text(title) })
    }
}

/// A label's lines, the first as the title and the rest as the subtitle under it.
private struct SubtitleLines: _VariadicView_MultiViewRoot {
    func body(children: _VariadicView.Children) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(Array(children.enumerated()), id: \.offset) { i, child in
                if i == 0 {
                    child
                } else {
                    child.font(.callout).foregroundColor(.secondary).fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }
}

extension View {
    /// `.formStyle(.grouped)`; Monterey's Form has no grouped style, so there it is the plain form with the
    /// grouped one's margins.
    @ViewBuilder func groupedForm() -> some View {
        if #available(macOS 13, *) {
            formStyle(.grouped)
        } else {
            padding(20)
        }
    }
}
