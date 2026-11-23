// Draws the install window's background: background.png (640 × 400 pt) and background@2x.png.
// Plain on purpose, like a page with no stylesheet: white, Times, a bold heading, an italic line, and ››› between the icons.
//
//   swift native/mac/dmg/background.swift   # rewrites both PNGs beside this file
//
// The layout matches settings.py: the app icon centred at (160, 210), Applications at (480, 210), 128 pt icons,
// Finder's labels under them. Change one, change the other.
import AppKit

let width: CGFloat = 640, height: CGFloat = 400
let iconY: CGFloat = 210  // icon centres, from the top as Finder counts
let dir = URL(fileURLWithPath: #filePath).deletingLastPathComponent()

/// Left-aligned at `x`, or centred when `x` is nil; `top` is the text's top edge, from the window's top.
func text(_ string: String, font: String, size: CGFloat, x: CGFloat?, top: CGFloat) {
  let s = NSAttributedString(
    string: string, attributes: [.font: NSFont(name: font, size: size)!, .foregroundColor: NSColor.black])
  let sz = s.size()
  s.draw(at: NSPoint(x: (x ?? (width - sz.width) / 2).rounded(), y: (height - top - sz.height).rounded()))
}

func draw(scale: CGFloat) -> Data {
  let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: Int(width * scale), pixelsHigh: Int(height * scale),
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
  rep.size = NSSize(width: width, height: height)
  NSGraphicsContext.saveGraphicsState()
  NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
  NSColor.white.setFill()
  NSRect(x: 0, y: 0, width: width, height: height).fill()
  // an unstyled web page's look: Times, a bold heading and an italic line, left-aligned
  text("Q(uick) Calc", font: "TimesNewRomanPS-BoldMT", size: 34, x: 36, top: 22)
  text("To install, drag Q Calc to Applications", font: "TimesNewRomanPS-ItalicMT", size: 20, x: 36, top: 74)
  text("›››", font: "TimesNewRomanPSMT", size: 44, x: nil, top: iconY - 30)
  NSGraphicsContext.restoreGraphicsState()
  return rep.representation(using: .png, properties: [:])!
}

for (scale, name) in [(CGFloat(1), "background.png"), (2, "background@2x.png")] {
  try! draw(scale: scale).write(to: dir.appendingPathComponent(name))
  print("Wrote \(dir.appendingPathComponent(name).path)")
}
