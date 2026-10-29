import AppKit

/// Files dragged onto the menu bar's Q (native only): the status item's window takes the drag (a window hands
/// the dragging calls to its delegate), the Q lights while files are over it, and the files go on to the bar.
final class StatusDrop: NSObject, NSWindowDelegate, NSDraggingDestination {
    private weak var button: NSStatusBarButton?
    private let dropped: ([URL]) -> Void

    init(_ button: NSStatusBarButton, dropped: @escaping ([URL]) -> Void) {
        self.button = button
        self.dropped = dropped
        super.init()
        button.window?.registerForDraggedTypes([.fileURL])
        button.window?.delegate = self
    }

    private func files(_ info: NSDraggingInfo) -> [URL] {
        info.draggingPasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL] ?? []
    }

    func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        guard !files(sender).isEmpty else { return [] }
        button?.highlight(true)
        return .copy
    }

    func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation {
        files(sender).isEmpty ? [] : .copy
    }

    func draggingExited(_ sender: NSDraggingInfo?) {
        button?.highlight(false)
    }

    func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        button?.highlight(false)
        let urls = files(sender)
        guard !urls.isEmpty else { return false }
        dropped(urls)
        return true
    }
}
