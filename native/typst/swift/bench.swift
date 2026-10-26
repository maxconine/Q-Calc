// The typst preview through the UniFFI bindings, the way the Mac app will call it: cold start, warm-up,
// per-keystroke renders, the corpus, memory, and a CGImage and a CGPath made from the results (written as PNGs
// for a look). Built and run by native/typst/scripts/measure.sh.
//
//   bench <corpus.jsonl> <out dir>

import CoreGraphics
import Darwin
import Foundation
import ImageIO
import UniformTypeIdentifiers

func footprintMB() -> Double {
    var info = task_vm_info_data_t()
    var count = mach_msg_type_number_t(MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
    let kr = withUnsafeMutablePointer(to: &info) {
        $0.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
            task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
        }
    }
    return kr == KERN_SUCCESS ? Double(info.phys_footprint) / 1_048_576 : -1
}

func ms(_ start: UInt64) -> Double { Double(DispatchTime.now().uptimeNanoseconds - start) / 1e6 }
func now() -> UInt64 { DispatchTime.now().uptimeNanoseconds }

func doc(_ math: String) -> String {
    "#import \"/macros.typ\": *\n#set page(width: auto, height: auto, margin: 2pt, fill: none)\n#set text(size: 18pt, fill: black)\n$ \(math) $\n"
}

func percentile(_ xs: [Double], _ p: Double) -> Double {
    let s = xs.sorted()
    return s.isEmpty ? 0 : s[min(s.count - 1, Int(Double(s.count) * p))]
}

func cgImage(_ img: TypstImage) -> CGImage {
    let provider = CGDataProvider(data: Data(img.rgba) as CFData)!
    return CGImage(width: Int(img.width), height: Int(img.height), bitsPerComponent: 8, bitsPerPixel: 32,
                   bytesPerRow: Int(img.width) * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                   bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                   provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)!
}

func cgPath(verbs: Data, points: [Float]) -> CGPath {
    let path = CGMutablePath()
    var i = 0
    func p() -> CGPoint { defer { i += 2 }; return CGPoint(x: CGFloat(points[i]), y: CGFloat(points[i + 1])) }
    for verb in verbs {
        switch verb {
        case 0: path.move(to: p())
        case 1: path.addLine(to: p())
        case 2: let c = p(); path.addQuadCurve(to: p(), control: c)
        case 3: let a = p(); let b = p(); path.addCurve(to: p(), control1: a, control2: b)
        default: path.closeSubpath()
        }
    }
    return path
}

/// Draws the paths into a bitmap context, the text colour standing in for `Foreground`.
func drawPaths(_ paths: TypstPaths, scale: CGFloat, foreground: CGColor) -> CGImage {
    let w = Int((CGFloat(paths.widthPt) * scale).rounded()), h = Int((CGFloat(paths.heightPt) * scale).rounded())
    let ctx = CGContext(data: nil, width: w, height: h, bitsPerComponent: 8, bytesPerRow: 0,
                        space: CGColorSpace(name: CGColorSpace.sRGB)!,
                        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    // page points are y down
    ctx.translateBy(x: 0, y: CGFloat(h))
    ctx.scaleBy(x: scale, y: -scale)
    for item in paths.paths {
        let color: CGColor
        switch item.paint {
        case .foreground: color = foreground
        case .color(let c): color = CGColor(srgbRed: CGFloat(c.r) / 255, green: CGFloat(c.g) / 255,
                                            blue: CGFloat(c.b) / 255, alpha: CGFloat(c.a) / 255)
        }
        ctx.addPath(cgPath(verbs: item.verbs, points: item.points))
        switch item.style {
        case .fill(let evenOdd):
            ctx.setFillColor(color)
            ctx.fillPath(using: evenOdd ? .evenOdd : .winding)
        case .stroke(let width, _, _, _, _, _):
            ctx.setStrokeColor(color)
            ctx.setLineWidth(CGFloat(width))
            ctx.strokePath()
        }
    }
    return ctx.makeImage()!
}

func writePNG(_ image: CGImage, _ path: String) {
    let dest = CGImageDestinationCreateWithURL(URL(fileURLWithPath: path) as CFURL, UTType.png.identifier as CFString, 1, nil)!
    CGImageDestinationAddImage(dest, image, nil)
    CGImageDestinationFinalize(dest)
}

let args = CommandLine.arguments
let corpusPath = args[1], outDir = args[2]
let base = footprintMB()
let light = TypstColor(r: 0x1d, g: 0x1d, b: 0x1f, a: 255)
let dark = TypstColor(r: 0xf5, g: 0xf5, b: 0xf7, a: 255)
func opts(_ c: TypstColor) -> TypstRenderOptions { TypstRenderOptions(pixelPerPt: 2, maxWidthPt: 600, maxHeightPt: 72, color: c) }

var t = now()
let typst = TypstRenderer()
let construct = ms(t)
t = now()
let first = try! typst.render(source: doc("x^2"), options: opts(light))
let coldFirst = ms(t)
let afterFirst = footprintMB()
print(String(format: "cold: renderer %.1f ms, first render %.1f ms (%dx%d)", construct, coldFirst, first.width, first.height))

let typst2 = TypstRenderer()
t = now()
let warm = typst2.warmUp()
print(String(format: "warmUp() on a second renderer in the same process: %.1f ms (%.1f ms wall)", warm, ms(t)))

// typing, one keystroke at a time; mid-edit prefixes fail like they do on the web
let typed = "frac(x^2 + 1, sqrt(2)) + sum_(k=1)^n k^2 - integral_0^1 e^(-t^2) dif t"
var keystroke: [Double] = []
var errors = 0
var prefix = ""
for ch in typed {
    prefix.append(ch)
    t = now()
    do { _ = try typst.render(source: doc(prefix), options: opts(light)) } catch { errors += 1 }
    keystroke.append(ms(t))
}
print(String(format: "keystrokes: %d (%d mid-edit errors), median %.2f ms, p90 %.2f ms, max %.2f ms",
             keystroke.count, errors, percentile(keystroke, 0.5), percentile(keystroke, 0.9), keystroke.max()!))

t = now()
_ = try! typst.render(source: doc(typed), options: opts(light))
let hit = ms(t)
t = now()
_ = try! typst.render(source: doc(typed), options: opts(dark))
let recolor = ms(t)
t = now()
let vec = try! typst.paths(source: doc(typed))
let pathMs = ms(t)
print(String(format: "cached image %.3f ms; same source in dark (no recompile) %.2f ms; paths %.2f ms (%d paths)",
                 hit, recolor, pathMs, vec.paths.count))

// the corpus: every source new to the renderer
let lines = try! String(contentsOfFile: corpusPath, encoding: .utf8).split(separator: "\n")
var corpus: [Double] = []
var corpusErrors = 0
t = now()
for line in lines {
    let obj = try! JSONSerialization.jsonObject(with: Data(line.utf8)) as! [String: Any]
    let s = now()
    do { _ = try typst.render(source: obj["doc"] as! String, options: opts(light)) } catch { corpusErrors += 1 }
    corpus.append(ms(s))
}
let total = ms(t)
print(String(format: "corpus: %d sources (%d errors) in %.0f ms, median %.2f ms, p90 %.2f ms, p99 %.2f ms, max %.2f ms",
             corpus.count, corpusErrors, total, percentile(corpus, 0.5), percentile(corpus, 0.9), percentile(corpus, 0.99), corpus.max()!))
print(String(format: "memory (phys_footprint): start %.1f MB, after first render %.1f MB, after the corpus %.1f MB",
             base, afterFirst, footprintMB()))
for _ in 0..<3 {
    for line in lines {
        let obj = try! JSONSerialization.jsonObject(with: Data(line.utf8)) as! [String: Any]
        _ = try? typst.render(source: obj["doc"] as! String, options: opts(dark))
    }
}
print(String(format: "memory after 3 more passes (%d renders): %.1f MB", lines.count * 3, footprintMB()))
typst.clearCaches()
print(String(format: "memory after clearCaches(): %.1f MB", footprintMB()))

let showcase = doc("frac(x^2 + 1, sqrt(2)) + sum_(k=1)^n k^2 = mat(1, 2; 3, 4) quad arrow.r.double quad x = plus.minus 2")
let img = try! typst.render(source: showcase, options: opts(light))
writePNG(cgImage(img), "\(outDir)/typst-light.png")
writePNG(cgImage(try! typst.render(source: showcase, options: opts(dark))), "\(outDir)/typst-dark.png")
writePNG(drawPaths(try! typst.paths(source: showcase), scale: 4,
                   foreground: CGColor(srgbRed: 0.11, green: 0.11, blue: 0.12, alpha: 1)), "\(outDir)/typst-paths.png")
print("wrote typst-light.png, typst-dark.png, typst-paths.png (\(img.width)x\(img.height))")
print("stats: \(typst.stats())")
