import CoreGraphics

// where a reference sheet (the periodic table, an identity sheet) pops up: off the bar, on the bar's screen.
// appkit's coordinates, y up
enum SheetPlace {
    static let margin: CGFloat = 12

    // how a sheet may give to fit beside the bar: an identity sheet scrolls, so it can get shorter; the periodic
    // table keeps its shape, so it scales
    enum Give {
        case none
        case height(min: CGFloat)
        case scale(min: CGFloat)
    }

    private enum Side: CaseIterable { case below, right, left, above }

    /// A sheet of `size` beside `bar` inside `visible`: below it, else right, left or above, whole if it fits on any
    /// side and given a little if not; when nothing fits even so, the side with the most room wins and the sheet is
    /// pulled onto the screen.
    static func beside(_ size: CGSize, bar: CGRect, visible: CGRect, give: Give = .none) -> CGRect {
        let size = CGSize(width: min(size.width, visible.width), height: min(size.height, visible.height))
        for side in Side.allCases where fits(size, room(side, bar, visible)) {
            return place(size, side, bar, visible)
        }
        for side in Side.allCases {
            if let given = given(size, room(side, bar, visible), give) { return place(given, side, bar, visible) }
        }
        let roomiest = Side.allCases.max { share(size, room($0, bar, visible)) < share(size, room($1, bar, visible)) }!
        return place(size, roomiest, bar, visible)
    }

    /// Whether a sheet where it is now would cover any of the bar, or sit off the bar's screen.
    static func needsMoving(_ frame: CGRect, bar: CGRect, visible: CGRect) -> Bool {
        frame.intersects(bar) || !visible.intersects(frame)
    }

    // the space on one side of the bar, a margin clear of it
    private static func room(_ side: Side, _ bar: CGRect, _ visible: CGRect) -> CGSize {
        switch side {
        case .below: return CGSize(width: visible.width, height: max(0, bar.minY - margin - visible.minY))
        case .above: return CGSize(width: visible.width, height: max(0, visible.maxY - bar.maxY - margin))
        case .right: return CGSize(width: max(0, visible.maxX - bar.maxX - margin), height: visible.height)
        case .left: return CGSize(width: max(0, bar.minX - margin - visible.minX), height: visible.height)
        }
    }

    private static func fits(_ size: CGSize, _ room: CGSize) -> Bool {
        size.width <= room.width && size.height <= room.height
    }

    private static func given(_ size: CGSize, _ room: CGSize, _ give: Give) -> CGSize? {
        switch give {
        case .none:
            return nil
        case let .height(least):
            return size.width <= room.width && room.height >= least ? CGSize(width: size.width, height: room.height) : nil
        case let .scale(least):
            let s = min(room.width / size.width, room.height / size.height)
            return s >= least ? CGSize(width: size.width * s, height: size.height * s) : nil
        }
    }

    private static func share(_ size: CGSize, _ room: CGSize) -> CGFloat {
        min(room.width / size.width, room.height / size.height)
    }

    // centred under or over the bar; level with its top beside it
    private static func place(_ size: CGSize, _ side: Side, _ bar: CGRect, _ visible: CGRect) -> CGRect {
        let (w, h) = (size.width, size.height)
        var r: CGRect
        switch side {
        case .below: r = CGRect(x: bar.midX - w / 2, y: bar.minY - margin - h, width: w, height: h)
        case .above: r = CGRect(x: bar.midX - w / 2, y: bar.maxY + margin, width: w, height: h)
        case .right: r = CGRect(x: bar.maxX + margin, y: bar.maxY - h, width: w, height: h)
        case .left: r = CGRect(x: bar.minX - margin - w, y: bar.maxY - h, width: w, height: h)
        }
        r.origin.x = max(visible.minX, min(r.minX, visible.maxX - w))
        r.origin.y = max(visible.minY, min(r.minY, visible.maxY - h))
        return r
    }
}
