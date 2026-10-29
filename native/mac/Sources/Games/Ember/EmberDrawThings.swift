import Foundation

// emberDraw.ts, the stage's things: pools, doors, signs, portals, switches, gates, movers, boxes, ice, version 2's
// tiles and things, fans, gems and beams. Line for line with the TS.

extension EmberRenderer {
    private var T: Double { Ember.TILE }

    // ---- pools

    func poolGradient(_ kind: Int) -> CanvasGradient {
        if let gr = poolGrad[kind] { return gr }
        let gr = CanvasGradient(.linear(CGPoint(x: 0, y: 0), CGPoint(x: 0, y: Ember.TILE)))
        let (top, bot): (RGB, RGB) = kind == Ember.T_LAVA ? (LAVA_TOP, LAVA_BOT) : kind == Ember.T_WATER ? (WATER_TOP, WATER_BOT)
            : kind == Ember.T_GOO ? (GOO_TOP, GOO_BOT) : kind == Ember.T_WALL ? (ICE_TOP, ICE_BOT) : (THIN_TOP, THIN_BOT)
        let water = kind == Ember.T_WATER || kind == Ember.T_THIN
        gr.addColorStop(0, css(top, water ? 0.88 : 1))
        if kind == Ember.T_LAVA { gr.addColorStop(0.35, css(LAVA_MID)) }
        gr.addColorStop(1, css(bot, water ? 0.95 : 1))
        poolGrad[kind] = gr
        return gr
    }

    func wave(_ kind: Int, _ wx: Double, _ t: Double) -> Double {
        if kind == Ember.T_LAVA { return sin(wx * 0.22 + t * 2.1) * 0.9 + sin(wx * 0.53 - t * 3.3) * 0.5 }
        if kind == Ember.T_GOO { return sin(wx * 0.17 + t * 1.3) * 0.7 + sin(wx * 0.41 - t * 1.9) * 0.35 }
        return sin(wx * 0.19 + t * 2.6) * 0.8 + sin(wx * 0.47 - t * 3.7) * 0.35
    }

    func drawPools(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ t: Double, _ dt: Double, _ opts: DrawOpts) {
        let tiles = level.tiles
        let vt = visibleTiles(vr, level.cols, level.rows, 1)
        let SINK = Ember.POOL_SINK
        if vt.y0 < vt.y1 && vt.x0 < vt.x1 {
            for ty in vt.y0..<vt.y1 {
                for tx in vt.x0..<vt.x1 {
                    let i = ty * level.cols + tx
                    let kind = tiles[i]
                    if kind != Ember.T_LAVA && kind != Ember.T_WATER && kind != Ember.T_GOO && kind != Ember.T_THIN { continue }
                    let x = Double(tx) * T, y = Double(ty) * T
                    if kind == Ember.T_THIN && world.tileAt(level, tx, ty) == Ember.T_WALL {
                        drawFrozen(g, level, cur, i, x, y, t)
                        continue
                    }
                    let open = ty == 0 || tiles[i - level.cols] != kind
                    if far {
                        g.fill(css(kind == Ember.T_LAVA ? LAVA_MID : kind == Ember.T_WATER ? WATER_BOT : kind == Ember.T_GOO ? GOO_BOT : THIN_BOT, kind == Ember.T_THIN ? 0.9 : 1))
                        g.fillRect(x, y + (open ? SINK : 0), T + 0.5, T - (open ? SINK : 0) + 0.5)
                        if open {
                            g.fill(css(kind == Ember.T_LAVA ? LAVA_TOP : kind == Ember.T_WATER ? WATER_TOP : kind == Ember.T_GOO ? GOO_TOP : THIN_TOP))
                            g.fillRect(x, y + SINK, T + 0.5, 2.5)
                        }
                        continue
                    }
                    g.save()
                    g.translate(x, y)
                    g.beginPath()
                    if open {
                        g.moveTo(0, SINK + wave(kind, x, t))
                        var lx = 2.0
                        while lx <= T {
                            g.lineTo(lx, SINK + wave(kind, x + lx, t))
                            lx += 2
                        }
                    } else {
                        g.moveTo(0, 0)
                        g.lineTo(T, 0)
                    }
                    g.lineTo(T, T)
                    g.lineTo(0, T)
                    g.closePath()
                    g.fill(poolGradient(kind))
                    g.fill()
                    if kind == Ember.T_LAVA || kind == Ember.T_GOO {
                        for b in 0..<2 {
                            let seed = hash01(Double(i * 7 + b))
                            let rate = kind == Ember.T_LAVA ? 0.55 : 0.35
                            let ph = (t * rate + seed).truncatingRemainder(dividingBy: 1)
                            let bx = 3 + hash01(Double(i * 13 + b) + floor(t * rate + seed) * 31) * (T - 6)
                            let by = T - 2 - ph * (T - SINK - 2) * (open ? 1.05 : 1.2)
                            let r = 0.8 + ph * 1.6
                            if by < (open ? SINK + 1 : 0) { continue }
                            g.fill(kind == Ember.T_LAVA ? Col(255, 236, 160, 0.8) : Col(214, 255, 150, 0.75))
                            g.beginPath()
                            g.arc(bx, by, r, 0, TAU)
                            g.fill()
                            if kind == Ember.T_GOO {
                                g.stroke(Col(40, 110, 20, 0.5))
                                g.lineWidth = 0.6
                                g.stroke()
                            }
                        }
                    } else if open {
                        for b in 0..<2 {
                            let ph = (t * 0.3 + hash01(Double(i * 5 + b))).truncatingRemainder(dividingBy: 1)
                            let gx = (ph * T * 1.4 + Double(b) * 9).truncatingRemainder(dividingBy: T)
                            g.fill(Col(255, 255, 255, ((0.5 * sin(ph * .pi)) * 1000).rounded() / 1000))
                            g.fillRect(gx, SINK + 3 + Double(b) * 4, 3, 0.8)
                        }
                    }
                    if open {
                        g.beginPath()
                        g.moveTo(0, SINK + wave(kind, x, t))
                        var lx = 2.0
                        while lx <= T {
                            g.lineTo(lx, SINK + wave(kind, x + lx, t))
                            lx += 2
                        }
                        g.stroke(kind == Ember.T_LAVA ? Col(255, 244, 190, 0.95) : kind == Ember.T_GOO ? Col(220, 255, 160, 0.9) : Col(235, 248, 255, 0.9))
                        g.lineWidth = 1.2
                        g.stroke()
                    }
                    g.restore()
                }
            }
        }
        beginGlow(g)
        var seen = 0
        for i in lavaTopsList {
            let tx = i % level.cols
            let ty = (i - tx) / level.cols
            if tx < vt.x0 || tx >= vt.x1 || ty < vt.y0 || ty >= vt.y1 { continue }
            seen += 1
            if far && tx & 1 != 0 { continue }
            let fl = 0.85 + 0.15 * sin(t * 3 + Double(tx))
            glow(g, Double(tx) * T + T / 2, Double(ty) * T + Ember.POOL_SINK + 2, far ? 36 : 24, css(LAVA_MID), 0.33 * fl)
        }
        endGlow(g)
        if seen > 0 && rnd() < Double(min(seen, 40)) * dt * (opts.reduceMotion ? 0.2 : 0.7) {
            let i = lavaTopsList[Int(floor(rnd() * Double(lavaTopsList.count)))]
            let tx = i % level.cols
            let ty = (i - tx) / level.cols
            if tx >= vt.x0 && tx < vt.x1 && ty >= vt.y0 && ty < vt.y1 {
                parts.spawn(PK_SPARK, rnd() < 0.5 ? C_CORE : C_FIRE2, Double(tx) * T + rand(2, 18), Double(ty) * T + Ember.POOL_SINK, rand(-12, 12), rand(-70, -30), rand(0.5, 0.9), rand(0.7, 1.1), 60, 0.5, true)
            }
        }
    }

    func drawFrozen(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ i: Int, _ x: Double, _ y: Double, _ t: Double) {
        let left = thinIndex[i].flatMap { at(cur.thin, $0) } ?? 999
        let warn = left < 40 ? 0.55 + 0.45 * abs(sin(t * 9)) : 1
        g.save()
        g.translate(x, y)
        g.globalAlpha = warn
        g.fill(poolGradient(Ember.T_WALL))
        let tx = Int(x / T), ty = Int(y / T)
        let lOpen = at(level.tiles, i - 1) != Ember.T_THIN && !isWall(level, tx - 1, ty)
        let rOpen = at(level.tiles, i + 1) != Ember.T_THIN && !isWall(level, tx + 1, ty)
        roundRectPath(g, 0, 0, T, T, lOpen ? 3 : 0, rOpen ? 3 : 0, 0, 0)
        g.fill()
        g.fill(Col(255, 255, 255, 0.85))
        g.fillRect(0, 0, T, 1.6)
        g.stroke(Col(255, 255, 255, 0.7))
        g.lineWidth = 0.8
        g.beginPath()
        let h = hash01(Double(i))
        g.moveTo(3 + h * 6, 4)
        g.lineTo(7 + h * 4, 9)
        g.lineTo(5 + h * 8, 15)
        g.moveTo(12 + h * 4, 3)
        g.lineTo(16, 7)
        g.stroke()
        g.stroke(css(pal.frost, 0.35))
        g.strokeRect(0.4, 0.4, T - 0.8, T - 0.8)
        g.restore()
        g.globalAlpha = 1
        if rnd() < 0.012 { parts.spawn(PK_STAR, C_WHITE, x + rand(3, 17), y + rand(2, 12), 0, 0, 0.6, 2.5, 0, 0, true) }
    }

    // ---- doors, signs, portals

    func drawDoors(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ t: Double, _ dt: Double) {
        let p = pal
        for i in 0..<2 {
            let d = level.doors[i]
            let fire = i == 0
            let c = fire ? p.fire : p.frost
            let c2 = fire ? p.fire2 : p.frost2
            let inDoor = cur.players[i].inDoor
            doorOpen[i] = approach(doorOpen[i], inDoor || cur.phase == "won" || cur.phase == "clear" ? 1 : 0, 6, dt)
            if !vis(d.x, d.y, d.w, d.h, 44) { continue }
            let o = doorOpen[i]
            let cx = d.x + d.w / 2
            beginGlow(g)
            glow(g, cx, d.y + d.h * 0.55, 26 + o * 14, css(c), 0.22 + o * 0.45 + 0.05 * sin(t * 2 + Double(i)))
            endGlow(g)
            let frame = p.dark ? mixRgb(p.wall, c, 0.25) : mixRgb(p.wall, c, 0.2)
            archPath(g, d.x - 2.5, d.y - 3, d.w + 5, d.h + 3)
            g.fill(css(frame))
            g.fill()
            g.stroke(css(mixRgb(frame, BLACK, 0.3), 0.6))
            g.lineWidth = 0.6
            g.beginPath()
            for k in 1..<6 {
                let ang = .pi + Double(k) * .pi / 6
                let ax = cx + cos(ang) * (d.w / 2)
                let ay = d.y + d.w / 2 + sin(ang) * (d.w / 2)
                g.moveTo(ax, ay)
                g.lineTo(cx + cos(ang) * (d.w / 2 + 2.5), d.y + d.w / 2 + sin(ang) * (d.w / 2 + 3))
            }
            var yy = d.y + d.w / 2 + 6
            while yy < d.y + d.h - 2 {
                g.moveTo(d.x - 2.5, yy)
                g.lineTo(d.x, yy)
                g.moveTo(d.x + d.w, yy)
                g.lineTo(d.x + d.w + 2.5, yy)
                yy += 6
            }
            g.stroke()
            archPath(g, d.x, d.y, d.w, d.h)
            let inner = CanvasGradient(.linear(CGPoint(x: 0, y: d.y), CGPoint(x: 0, y: d.y + d.h)))
            let shut = p.dark ? mixRgb(c, BLACK, 0.72) : mixRgb(c, p.ink, 0.55)
            inner.addColorStop(0, css(mixRgb(shut, c2, o * 0.9)))
            inner.addColorStop(1, css(mixRgb(mixRgb(shut, BLACK, 0.2), c, o * 0.8)))
            g.fill(inner)
            g.fill()
            g.stroke(css(c, 0.45 + o * 0.4))
            g.lineWidth = 0.9
            archPath(g, d.x + 0.5, d.y + 0.5, d.w - 1, d.h - 0.5)
            g.stroke()
            if o > 0.02 {
                g.fill(css(c2, 0.35 * o))
                g.beginPath()
                g.ellipse(cx, d.y + d.h, d.w * 0.9, 2.2, 0, 0, TAU)
                g.fill()
            }
            g.fill(css(c))
            g.beginPath()
            g.moveTo(cx - 2.5, d.y - 3)
            g.lineTo(cx + 2.5, d.y - 3)
            g.lineTo(cx + 1.6, d.y + 1)
            g.lineTo(cx - 1.6, d.y + 1)
            g.closePath()
            g.fill()
            g.fill(css(c, 0.8))
            g.fillRect(d.x - 3, d.y + d.h - 1.5, d.w + 6, 1.5)
            let gy = d.y + d.h * 0.42 + sin(t * 2.2 + Double(i)) * 0.8
            g.globalAlpha = 0.75 + o * 0.25
            if fire {
                g.fill(css(o > 0.5 ? WHITE : c2))
                flameGlyph(g, cx, gy, 11)
                g.fill()
                g.fill(css(o > 0.5 ? c2 : c))
                flameGlyph(g, cx + 0.3, gy + 2, 5)
                g.fill()
            } else {
                g.stroke(css(o > 0.5 ? WHITE : c2))
                g.lineWidth = 1.1
                g.lineCap = .round
                flakeGlyph(g, cx, gy, 4.6)
                g.stroke()
            }
            g.globalAlpha = 1
        }
    }

    func drawSigns(_ g: Canvas2D, _ level: EStage, _ t: Double) {
        if level.signs.isEmpty { return }
        let p = pal
        g.font = .systemFont(ofSize: 9)
        g.textAlign = .center
        g.textBaseline = .middle
        for (i, s) in level.signs.enumerated() {
            if !vis(s.x - 100, s.y - 10, 200, 20) { continue }
            var w = textW[s.text]
            if w == nil {
                w = g.measureText(s.text)
                textW[s.text] = w
            }
            let cx = s.x
            let cy = s.y + sin(t * 1.6 + Double(i)) * 0.8
            let bw = w! + 10
            rr(g, cx - bw / 2, cy - 7, bw, 14, 7)
            g.fill(css(p.bg, p.dark ? 0.82 : 0.88))
            g.fill()
            g.stroke(css(p.ink, 0.12))
            g.lineWidth = 0.8
            g.stroke()
            g.fill(css(p.ink, 0.78))
            g.fillText(s.text, cx, cy + 0.5)
        }
    }

    func drawPortals(_ g: Canvas2D, _ level: EStage, _ t: Double, _ opts: DrawOpts) {
        for (i, pt) in level.portals.enumerated() {
            if !vis(pt.x, pt.y, pt.w, pt.h, 34) { continue }
            let pc = at(portalCol, i) ?? 0
            let col = PORTAL_COLORS[pc]
            let cx = pt.x + pt.w / 2, cy = pt.y + pt.h / 2
            let rx = pt.w / 2 - 1, ry = pt.h / 2 - 1
            beginGlow(g)
            glow(g, cx, cy, 30, css(col), 0.45 + 0.1 * sin(t * 3 + Double(i)))
            endGlow(g)
            let gr = CanvasGradient(.radial(CGPoint(x: cx, y: cy), 0, CGPoint(x: cx, y: cy), ry))
            gr.addColorStop(0, css(mixRgb(col, BLACK, 0.75)))
            gr.addColorStop(0.7, css(mixRgb(col, BLACK, 0.35)))
            gr.addColorStop(1, css(col))
            g.fill(gr)
            g.beginPath()
            g.ellipse(cx, cy, rx, ry, 0, 0, TAU)
            g.fill()
            g.lineWidth = 1.2
            for r in 0..<3 {
                let s = 1 - Double(r) * 0.27
                g.stroke(css(mixRgb(col, WHITE, 0.25 + Double(r) * 0.25), 0.9 - Double(r) * 0.2))
                g.setLineDash([5 * s, 4 * s])
                g.lineDashOffset = (r % 2 != 0 ? -1 : 1) * t * (18 + Double(r) * 8)
                g.beginPath()
                g.ellipse(cx, cy, rx * s, ry * s, 0, 0, TAU)
                g.stroke()
            }
            g.setLineDash([])
            if rnd() < (opts.reduceMotion ? 0.03 : 0.12) {
                let ang = rnd() * TAU
                parts.spawn(PK_DOT, C_PORTAL + pc, cx + cos(ang) * rx * 1.3, cy + sin(ang) * ry * 1.1, -cos(ang) * 14, -sin(ang) * 26, 0.6, 1, 0, 0.5, true)
            }
        }
    }

    // ---- switches

    func drawPlates(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ t: Double) {
        for (i, pl) in level.plates.enumerated() {
            if !vis(pl.x, pl.y, pl.w, pl.h) { continue }
            let on = cur.plates.flatMap { at($0, i) } ?? cur.on.contains(pl.ch)
            let col = CHANNEL_COLORS[channelIndex(pl.ch)]
            let floor = pl.y + pl.h
            if pl.ceil {
                g.save()
                g.translate(0, 2 * pl.y + pl.h)
                g.scale(1, -1)
            }
            let h: Double = on ? 1 : 3
            let metal = pal.dark ? mixRgb(METAL, BLACK, 0.3) : METAL
            if on {
                beginGlow(g)
                glow(g, pl.x + pl.w / 2, floor - 2, pl.w * 0.6 + 8, col, 0.55 + 0.08 * sin(t * 6))
                endGlow(g)
            }
            g.fill(col)
            rr(g, pl.x + 2, floor - 2.5 - h, pl.w - 4, h + 1, 1)
            g.fill()
            g.fill(on ? Col(255, 255, 255, 0.6) : Col(255, 255, 255, 0.3))
            g.fillRect(pl.x + 3, floor - 2.5 - h, pl.w - 6, 0.8)
            g.fill(css(metal))
            roundRectPath(g, pl.x, floor - 2.5, pl.w, 2.5, 1.2, 1.2, 0, 0)
            g.fill()
            g.fill(on ? col : css(mixRgb(metal, BLACK, 0.3)))
            g.fillRect(pl.x + 1.5, floor - 1.4, pl.w - 3, 0.7)
            if pl.ceil { g.restore() }
        }
    }

    func drawButtons(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ t: Double) {
        for (i, b) in level.buttons.enumerated() {
            if !vis(b.x, b.y, b.w, b.h) { continue }
            let left = at(cur.buttons, i) ?? 0
            let frac = b.ticks > 0 ? clamp(left / b.ticks, 0, 1) : 0
            let on = left > 0
            let col = CHANNEL_COLORS[channelIndex(b.ch)]
            let cx = b.x + b.w / 2
            let floor = b.y + b.h
            if b.ceil {
                g.save()
                g.translate(0, 2 * b.y + b.h)
                g.scale(1, -1)
            }
            g.fill(css(METAL))
            rr(g, b.x + 1, floor - 2.5, b.w - 2, 2.5, 1)
            g.fill()
            let dome: Double = on ? 2 : 4.2
            g.fill(col)
            g.beginPath()
            g.ellipse(cx, floor - 2.5, min(b.w / 2 - 3, 7), dome, 0, .pi, 0)
            g.fill()
            g.fill(Col(255, 255, 255, 0.45))
            g.beginPath()
            g.ellipse(cx - 2, floor - 2.5 - dome * 0.55, 2, dome * 0.25, 0, 0, TAU)
            g.fill()
            if on {
                let ay = floor - 13
                beginGlow(g)
                glow(g, cx, floor - 3, 14, col, 0.4)
                endGlow(g)
                g.lineCap = .round
                g.stroke(css(pal.ink, 0.15))
                g.lineWidth = 1.6
                g.beginPath()
                g.arc(cx, ay, 4, 0, TAU)
                g.stroke()
                g.stroke(col)
                g.lineWidth = frac < 0.25 && sin(t * 20) > 0 ? 2.4 : 1.8
                g.beginPath()
                g.arc(cx, ay, 4, -.pi / 2, -.pi / 2 + TAU * frac)
                g.stroke()
            }
            if b.ceil { g.restore() }
        }
    }

    func drawLevers(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ dt: Double) {
        for (i, lv) in level.levers.enumerated() {
            let on = at(cur.levers, i) ?? lv.on
            if leverAng.indices.contains(i) { leverAng[i] = approach(leverAng[i], on ? 1 : -1, 14, dt) }
            if !vis(lv.x, lv.y, lv.w, lv.h) { continue }
            let ang = (at(leverAng, i) ?? 0) * 0.65
            let col = CHANNEL_COLORS[channelIndex(lv.ch)]
            let cx = lv.x + lv.w / 2
            let by = lv.y + lv.h - 1
            let hx = cx + sin(ang) * 12
            let hy = by - 3 - cos(ang) * 12
            g.lineCap = .round
            g.stroke(css(pal.dark ? mixRgb(METAL, WHITE, 0.2) : mixRgb(METAL, BLACK, 0.15)))
            g.lineWidth = 2
            g.beginPath()
            g.moveTo(cx, by - 3)
            g.lineTo(hx, hy)
            g.stroke()
            g.fill(col)
            g.beginPath()
            g.arc(hx, hy, 2.6, 0, TAU)
            g.fill()
            g.fill(Col(255, 255, 255, 0.55))
            g.beginPath()
            g.arc(hx - 0.8, hy - 0.8, 0.9, 0, TAU)
            g.fill()
            g.fill(css(pal.dark ? mixRgb(METAL, BLACK, 0.3) : METAL))
            g.beginPath()
            g.ellipse(cx, by, 6.5, 5, 0, .pi, 0)
            g.fill()
            g.fill(on ? col : css(pal.wallLo))
            g.beginPath()
            g.arc(cx, by - 2, 1.4, 0, TAU)
            g.fill()
            if on {
                beginGlow(g)
                glow(g, cx, by - 2, 8, col, 0.6)
                endGlow(g)
            }
        }
    }

    func drawSensors(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ t: Double) {
        for (i, s) in level.sensors.enumerated() {
            if !vis(s.x, s.y, s.w, s.h) { continue }
            let lit = cur.sensors.flatMap { at($0, i) } ?? cur.on.contains(s.ch)
            let col = CHANNEL_COLORS[channelIndex(s.ch)]
            let cx = s.x + s.w / 2, cy = s.y + s.h / 2
            g.fill(css(pal.dark ? mixRgb(METAL, BLACK, 0.35) : METAL))
            rr(g, s.x + 2, s.y + 2, s.w - 4, s.h - 4, 4)
            g.fill()
            g.stroke(col)
            g.lineWidth = 1.2
            g.stroke()
            g.fill(lit ? css(mixRgb(rgbOf(col), WHITE, 0.5)) : css(BLACK, 0.55))
            g.beginPath()
            g.arc(cx, cy, 4, 0, TAU)
            g.fill()
            g.fill(col)
            g.beginPath()
            g.arc(cx, cy, lit ? 2.4 : 1.4, 0, TAU)
            g.fill()
            if lit {
                beginGlow(g)
                glow(g, cx, cy, 16, col, 0.7 + 0.15 * sin(t * 8))
                endGlow(g)
            }
        }
    }

    func drawEmitters(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ t: Double) {
        for em in level.emitters {
            if !vis(em.x - 10, em.y - 10, 20, 20) { continue }
            let on = condHolds(em.when, cur.on, cur.lit)
            let ch = condChannels(em.when).first
            let rot: Double = em.dir == "right" ? 0 : em.dir == "down" ? .pi / 2 : em.dir == "left" ? .pi : -.pi / 2
            g.save()
            g.translate(em.x, em.y)
            g.rotate(rot)
            g.fill(css(pal.dark ? mixRgb(METAL, BLACK, 0.3) : METAL))
            rr(g, -8, -7, 13, 14, 3)
            g.fill()
            if let ch {
                g.fill(channelColor(ch))
                g.fillRect(-8, -7, 2.5, 14)
            }
            g.fill(css(pal.dark ? mixRgb(METAL, WHITE, 0.15) : mixRgb(METAL, BLACK, 0.2)))
            g.fillRect(5, -4, 3.5, 8)
            g.fill(on ? colors[C_CORE] : css(BLACK, 0.5))
            g.beginPath()
            g.arc(8.5, 0, 2.6, -.pi / 2, .pi / 2)
            g.fill()
            g.restore()
            if on {
                let dx: Double = em.dir == "right" ? 8 : em.dir == "left" ? -8 : 0
                let dy: Double = em.dir == "down" ? 8 : em.dir == "up" ? -8 : 0
                beginGlow(g)
                glow(g, em.x + dx, em.y + dy, 12, css(pal.star), 0.7 + 0.1 * sin(t * 14))
                endGlow(g)
            }
        }
    }

    func drawMirrors(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ dt: Double) {
        for (i, m) in level.mirrors.enumerated() {
            let slant = at(cur.mirrors, i) ?? m.slant
            if mirrorAng.indices.contains(i) { mirrorAng[i] = approach(mirrorAng[i], slant == "/" ? -.pi / 4 : .pi / 4, 16, dt) }
            if !vis(m.x - 12, m.y - 12, 24, 24) { continue }
            g.fill(css(pal.dark ? mixRgb(METAL, BLACK, 0.3) : METAL))
            g.fillRect(m.x - 1, m.y, 2, T / 2)
            g.beginPath()
            g.arc(m.x, m.y, 2.4, 0, TAU)
            g.fill()
            g.save()
            g.translate(m.x, m.y)
            g.rotate(at(mirrorAng, i) ?? 0)
            g.fill(css(pal.dark ? mixRgb(METAL, BLACK, 0.45) : mixRgb(METAL, BLACK, 0.25)))
            rr(g, -12, -2.2, 24, 4.4, 2)
            g.fill()
            let sheen = CanvasGradient(.linear(CGPoint(x: -12, y: 0), CGPoint(x: 12, y: 0)))
            sheen.addColorStop(0, Col(210, 230, 255, 0.95))
            sheen.addColorStop(0.5, Col(255, 255, 255, 1))
            sheen.addColorStop(1, Col(180, 205, 240, 0.95))
            g.fill(sheen)
            rr(g, -11, -2.2, 22, 2.2, 1.1)
            g.fill()
            g.restore()
        }
    }

    // ---- solid moving things

    func drawGates(_ g: Canvas2D, _ level: EStage, _ cur: EState) {
        let p = pal
        for (i, home) in level.gates.enumerated() {
            if !vis(home.x, home.y, home.w, home.h, 4) { continue }
            let r = world.gateRect(i)
            let chs = condChannels(home.open)
            g.save()
            g.beginPath()
            g.rect(home.x, home.y, home.w, home.h)
            g.clip()
            let bar = p.dark ? mixRgb(METAL, BLACK, 0.15) : METAL
            g.fill(css(mixRgb(bar, BLACK, 0.35)))
            rr(g, r.x, r.y, r.w, r.h, 2)
            g.fill()
            let vertical = r.h >= r.w
            g.fill(css(bar))
            if vertical {
                var bx = r.x + 2
                while bx < r.x + r.w - 1 { g.fillRect(bx, r.y + 1, 2.5, r.h - 2); bx += 5 }
                g.fill(Col(255, 255, 255, 0.28))
                bx = r.x + 2
                while bx < r.x + r.w - 1 { g.fillRect(bx, r.y + 1, 0.8, r.h - 2); bx += 5 }
            } else {
                var by = r.y + 2
                while by < r.y + r.h - 1 { g.fillRect(r.x + 1, by, r.w - 2, 2.5); by += 5 }
                g.fill(Col(255, 255, 255, 0.28))
                by = r.y + 2
                while by < r.y + r.h - 1 { g.fillRect(r.x + 1, by, r.w - 2, 0.8); by += 5 }
            }
            let n = max(1, chs.count)
            for c in 0..<n {
                g.fill(c < chs.count ? channelColor(chs[c]) : css(p.muted))
                if vertical { g.fillRect(r.x, r.y + r.h - 7 - Double(c) * 4, r.w, 3) } else { g.fillRect(r.x + r.w - 7 - Double(c) * 4, r.y, 3, r.h) }
            }
            g.stroke(css(BLACK, p.dark ? 0.45 : 0.25))
            g.lineWidth = 0.8
            rr(g, r.x + 0.4, r.y + 0.4, r.w - 0.8, r.h - 0.8, 2)
            g.stroke()
            g.restore()
        }
    }

    func drawMoverRails(_ g: Canvas2D, _ level: EStage) {
        g.lineCap = .round
        for m in level.movers {
            if !vis(min(m.x, m.to.x), min(m.y, m.to.y), abs(m.to.x - m.x) + m.w, abs(m.to.y - m.y) + m.h, 4) { continue }
            let ch = condChannels(m.when).first
            let col = ch.map(channelColor) ?? css(pal.muted)
            g.stroke(col)
            g.globalAlpha = 0.45
            g.lineWidth = 1.5
            g.setLineDash([0.1, 5])
            g.beginPath()
            g.moveTo(m.x + m.w / 2, m.y + m.h / 2)
            g.lineTo(m.to.x + m.w / 2, m.to.y + m.h / 2)
            g.stroke()
            g.setLineDash([])
            g.fill(col)
            g.beginPath()
            g.arc(m.x + m.w / 2, m.y + m.h / 2, 1.6, 0, TAU)
            g.arc(m.to.x + m.w / 2, m.to.y + m.h / 2, 1.6, 0, TAU)
            g.fill()
            g.globalAlpha = 1
        }
    }

    func drawMovers(_ g: Canvas2D, _ level: EStage, _ prev: EState?, _ cur: EState, _ a: Double) {
        let p = pal
        for (i, m) in level.movers.enumerated() {
            guard let c = at(cur.movers, i) else { continue }
            let q = prev.flatMap { at($0.movers, i) }
            let isFar = q == nil || abs(q!.x - c.x) > 40 || abs(q!.y - c.y) > 40
            let x = isFar ? c.x : q!.x + (c.x - q!.x) * a
            let y = isFar ? c.y : q!.y + (c.y - q!.y) * a
            if !vis(x, y, m.w, m.h, 4) { continue }
            let ch = condChannels(m.when).first
            let body = p.dark ? mixRgb(p.wall, METAL, 0.5) : mixRgb(p.wall, METAL, 0.45)
            g.fill(css(BLACK, p.dark ? 0.3 : 0.1))
            rr(g, x + 1, y + 2, m.w, m.h, 3)
            g.fill()
            g.fill(css(body))
            rr(g, x, y, m.w, m.h, 3)
            g.fill()
            g.fill(css(p.dark ? mixRgb(body, WHITE, 0.2) : mixRgb(body, WHITE, 0.5)))
            g.fillRect(x + 1, y, m.w - 2, 2)
            g.fill(ch.map(channelColor) ?? css(p.muted))
            g.fillRect(x + 3, y + 3, m.w - 6, 2)
            g.fill(css(mixRgb(body, BLACK, 0.35)))
            var rx = x + 4
            while rx < x + m.w - 2 {
                g.beginPath()
                g.arc(rx, y + m.h - 4, 0.9, 0, TAU)
                g.fill()
                rx += 10
            }
        }
    }

    func drawBoxes(_ g: Canvas2D, _ prev: EState?, _ cur: EState, _ a: Double) {
        let p = pal
        let wood = p.dark ? mixRgb(WOOD, BLACK, 0.1) : WOOD
        for (i, c) in cur.boxes.enumerated() {
            let q = prev.flatMap { at($0.boxes, i) }
            let isFar = q == nil || abs(q!.x - c.x) > 40 || abs(q!.y - c.y) > 40
            let x = isFar ? c.x : q!.x + (c.x - q!.x) * a
            let y = isFar ? c.y : q!.y + (c.y - q!.y) * a
            let s = T
            if !vis(x, y, s, s, 4) { continue }
            g.fill(css(BLACK, p.dark ? 0.3 : 0.1))
            rr(g, x + 1, y + 2, s, s, 2)
            g.fill()
            g.fill(css(wood))
            rr(g, x, y, s, s, 2)
            g.fill()
            g.stroke(css(WOOD_DARK, 0.7))
            g.lineWidth = 0.8
            g.beginPath()
            g.moveTo(x + 2, y + s / 3)
            g.lineTo(x + s - 2, y + s / 3)
            g.moveTo(x + 2, y + 2 * s / 3)
            g.lineTo(x + s - 2, y + 2 * s / 3)
            g.stroke()
            g.stroke(css(mixRgb(wood, WHITE, 0.15)))
            g.lineWidth = 2
            g.beginPath()
            g.moveTo(x + 3, y + s - 3)
            g.lineTo(x + s - 3, y + 3)
            g.stroke()
            g.stroke(css(WOOD_DARK))
            g.lineWidth = 1.4
            rr(g, x + 0.7, y + 0.7, s - 1.4, s - 1.4, 2)
            g.stroke()
            g.fill(Col(255, 255, 255, 0.25))
            g.fillRect(x + 1.5, y + 1.2, s - 3, 1)
        }
    }

    func drawIce(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ t: Double) {
        for (i, r) in level.ice.enumerated() {
            let p = at(cur.ice, i) ?? 0
            let melting = p > (at(iceLast, i) ?? 0)
            if iceLast.indices.contains(i) { iceLast[i] = p }
            if p >= 1 || !vis(r.x, r.y, r.w, r.h, 4) { continue }
            let jx = melting ? sin(t * 60 + Double(i)) * 0.6 : 0
            let x = r.x + jx
            g.save()
            g.globalAlpha = 0.92 - p * 0.35
            let gr = CanvasGradient(.linear(CGPoint(x: x, y: r.y), CGPoint(x: x + r.w, y: r.y + r.h)))
            gr.addColorStop(0, css(ICE_TOP, 0.92))
            gr.addColorStop(1, css(mixRgb(ICE_BOT, pal.frost, 0.3), 0.85))
            g.fill(gr)
            rr(g, x, r.y, r.w, r.h, 2.5)
            g.fill()
            g.stroke(Col(255, 255, 255, 0.55))
            g.lineWidth = 0.7
            g.beginPath()
            var ty = r.y + T
            while ty < r.y + r.h {
                g.moveTo(x + 1, ty)
                g.lineTo(x + r.w - 1, ty)
                ty += T
            }
            var tx = x + T
            while tx < x + r.w {
                g.moveTo(tx, r.y + 1)
                g.lineTo(tx, r.y + r.h - 1)
                tx += T
            }
            g.stroke()
            g.stroke(Col(255, 255, 255, 0.85))
            g.lineWidth = 1.2
            g.lineCap = .round
            g.beginPath()
            g.moveTo(x + 3, r.y + 7)
            g.lineTo(x + 7, r.y + 3)
            g.moveTo(x + 3, r.y + 11)
            g.lineTo(x + 5, r.y + 9)
            g.stroke()
            let cracks = Int(ceil(p * 7))
            if cracks > 0 {
                g.stroke(css(pal.frost, 0.85))
                g.lineWidth = 0.8
                g.beginPath()
                for c in 0..<cracks {
                    var cx = x + hash01(Double(i * 41 + c)) * r.w
                    var cy = r.y + hash01(Double(i * 43 + c + 7)) * r.h
                    g.moveTo(cx, cy)
                    for s in 0..<3 {
                        cx += (hash01(Double(i * 47 + c * 5 + s)) - 0.5) * 10
                        cy += (hash01(Double(i * 53 + c * 5 + s)) - 0.5) * 10
                        g.lineTo(clamp(cx, x + 1, x + r.w - 1), clamp(cy, r.y + 1, r.y + r.h - 1))
                    }
                }
                g.stroke()
            }
            g.stroke(css(pal.frost, 0.6))
            g.lineWidth = 1
            rr(g, x + 0.5, r.y + 0.5, r.w - 1, r.h - 1, 2.5)
            g.stroke()
            g.restore()
            if melting && rnd() < 0.3 { parts.spawn(PK_DOT, C_WATER, x + rand(1, r.w - 1), r.y + r.h - 1, 0, rand(10, 30), 0.4, 1, 300, 0) }
        }
    }

    // ---- version 2's tiles

    func drawTiles(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ t: Double, _ opts: DrawOpts) {
        let tiles = level.tiles
        let vt = visibleTiles(vr, level.cols, level.rows, 1)
        let p = pal
        let lit = 1 - darkA
        guard vt.y0 < vt.y1, vt.x0 < vt.x1 else { return }
        for ty in vt.y0..<vt.y1 {
            for tx in vt.x0..<vt.x1 {
                let i = ty * level.cols + tx
                let kind = tiles[i]
                if kind < Ember.T_TRAMP { continue }
                let x = Double(tx) * T, y = Double(ty) * T
                if kind == Ember.T_TRAMP {
                    drawTramp(g, level, i, tx, ty, t)
                } else if kind == Ember.T_ONEWAY {
                    let l = tx > 0 && tiles[i - 1] != 0 && tiles[i - 1] != Ember.T_CRUMBLE
                    let r = tx < level.cols - 1 && tiles[i + 1] != 0 && tiles[i + 1] != Ember.T_CRUMBLE
                    let H = Ember.ONEWAY_H
                    let wood = p.dark ? mixRgb(WOOD, p.wall, 0.25) : mixRgb(WOOD, p.wallHi, 0.2)
                    g.fill(css(BLACK, p.dark ? 0.28 : 0.1))
                    g.fillRect(x + 1, y + 2, T, H)
                    g.fill(css(wood))
                    roundRectPath(g, x, y, T + (r ? 0.4 : 0), H, l ? 0 : 2.5, r ? 0 : 2.5, r ? 0 : 2.5, l ? 0 : 2.5)
                    g.fill()
                    g.fill(css(mixRgb(wood, WHITE, p.dark ? 0.35 : 0.5)))
                    g.fillRect(x + (l ? 0 : 1.5), y, T - (l ? 0 : 1.5) - (r ? 0 : 1.5), 1.4)
                    g.fill(css(WOOD_DARK, 0.7))
                    g.fillRect(x + (l ? 0 : 1.5), y + H - 1.2, T - (l ? 0 : 1.5) - (r ? 0 : 1.5), 1.2)
                    if !far {
                        g.stroke(css(p.ink, 0.2))
                        g.lineWidth = 1
                        g.beginPath()
                        g.moveTo(x + 7, y + 12.5)
                        g.lineTo(x + 10, y + 9.5)
                        g.lineTo(x + 13, y + 12.5)
                        g.stroke()
                    }
                } else if kind == Ember.T_CRUMBLE {
                    drawCrumble(g, cur, i, x, y, t, opts)
                } else {
                    let sun = kind == Ember.T_LIGHT
                    let a = sun ? lit : 1 - lit
                    let col = sun ? SUN : MOON
                    let cx = x + T / 2, cy = y + T / 2
                    if a < 0.97 {
                        g.globalAlpha = 1 - a
                        g.fill(css(col, p.dark ? 0.07 : 0.1))
                        rr(g, x + 1.5, y + 1.5, T - 3, T - 3, 3)
                        g.fill()
                        g.stroke(css(p.dark ? col : mixRgb(col, BLACK, 0.3), 0.75))
                        g.lineWidth = 1
                        g.setLineDash([3, 2.5])
                        g.stroke()
                        g.setLineDash([])
                        if !far { lightGlyph(g, sun, cx, cy, css(p.dark ? col : mixRgb(col, BLACK, 0.3), 0.6)) }
                        g.globalAlpha = 1
                    }
                    if a > 0.03 {
                        let k = 0.8 + 0.2 * a
                        g.save()
                        g.translate(cx, cy)
                        g.scale(k, k)
                        g.globalAlpha = a
                        g.fill(css(BLACK, p.dark ? 0.3 : 0.1))
                        rr(g, -T / 2 + 1, -T / 2 + 2, T, T, 3)
                        g.fill()
                        let body = sun ? mixRgb(SUN, WHITE, 0.35) : p.dark ? mixRgb(MOON, BLACK, 0.45) : mixRgb(MOON, BLACK, 0.3)
                        g.fill(css(body))
                        rr(g, -T / 2, -T / 2, T, T, 3)
                        g.fill()
                        g.fill(css(WHITE, sun ? 0.7 : 0.22))
                        g.fillRect(-T / 2 + 2, -T / 2, T - 4, 2.2)
                        g.fill(css(sun ? mixRgb(SUN, LAVA_MID, 0.5) : BLACK, sun ? 0.55 : 0.35))
                        g.fillRect(-T / 2 + 2, T / 2 - 2.2, T - 4, 2.2)
                        g.stroke(css(sun ? mixRgb(SUN, LAVA_MID, 0.55) : mixRgb(MOON, WHITE, 0.3), 0.9))
                        g.lineWidth = 1
                        rr(g, -T / 2 + 0.5, -T / 2 + 0.5, T - 1, T - 1, 3)
                        g.stroke()
                        if !far { lightGlyph(g, sun, 0, 0, sun ? css(mixRgb(SUN, LAVA_BOT, 0.55)) : css(mixRgb(MOON, WHITE, 0.6))) }
                        g.restore()
                        g.globalAlpha = 1
                    }
                }
            }
        }
    }

    func lightGlyph(_ g: Canvas2D, _ sun: Bool, _ cx: Double, _ cy: Double, _ color: Col) {
        g.fill(color)
        g.stroke(color)
        if sun {
            g.beginPath()
            g.arc(cx, cy, 2.4, 0, TAU)
            g.fill()
            g.lineWidth = 1
            g.lineCap = .round
            g.beginPath()
            for k in 0..<8 {
                let an = Double(k) * TAU / 8
                g.moveTo(cx + cos(an) * 4, cy + sin(an) * 4)
                g.lineTo(cx + cos(an) * 5.6, cy + sin(an) * 5.6)
            }
            g.stroke()
        } else {
            g.beginPath()
            g.arc(cx, cy, 4.4, .pi * 0.35, .pi * 1.65)
            g.arc(cx + 2.6, cy, 3.6, .pi * 1.42, .pi * 0.58, true)
            g.closePath()
            g.fill()
        }
    }

    func drawTramp(_ g: Canvas2D, _ level: EStage, _ i: Int, _ tx: Int, _ ty: Int, _ t: Double) {
        let p = pal
        let x = Double(tx) * T, y = Double(ty) * T
        var d = 0.0
        if let at = trampAt[i] {
            let e = t - at
            if e > 0.6 { trampAt[i] = nil } else { d = 4.5 * exp(-e * 7) * cos(e * 34) }
        }
        let l = tx > 0 && level.tiles[i - 1] == Ember.T_TRAMP
        let r = tx < level.cols - 1 && level.tiles[i + 1] == Ember.T_TRAMP
        let base = p.dark ? mixRgb(METAL, BLACK, 0.45) : mixRgb(METAL, BLACK, 0.2)
        g.fill(css(base))
        roundRectPath(g, x, y + 5, T + (r ? 0.4 : 0), T - 5, 0, 0, r ? 0 : 3, l ? 0 : 3)
        g.fill()
        g.fill(css(BLACK, 0.4))
        g.fillRect(x + 4, y + 7, T - 8, 9)
        g.stroke(css(p.dark ? mixRgb(METAL, WHITE, 0.4) : mixRgb(METAL, WHITE, 0.6)))
        g.lineWidth = 1.2
        g.beginPath()
        let top = y + 6 + max(-1, d)
        for k in 0...4 {
            let yy = top + (y + 16 - top) * Double(k) / 4
            let xx = x + (k % 2 != 0 ? 14.5 : 5.5)
            if k == 0 { g.moveTo(xx, yy) } else { g.lineTo(xx, yy) }
        }
        g.stroke()
        let pad = { (py: Double, flip: Bool) in
            let h = 5.0
            g.fill(css(TRAMP))
            roundRectPath(g, x - (l ? 0 : 0.5), py, self.T + (l ? 0 : 0.5) + (r ? 0.4 : 0.5), h, l || flip ? 0 : 2.5, r || flip ? 0 : 2.5, r || !flip ? 0 : 2.5, l || !flip ? 0 : 2.5)
            g.fill()
            g.fill(css(WHITE, 0.85))
            g.fillRect(x + 2, flip ? py + h - 1.6 : py, 5, 1.6)
            g.fillRect(x + 13, flip ? py + h - 1.6 : py, 5, 1.6)
            g.fill(css(mixRgb(TRAMP, BLACK, 0.4), 0.8))
            g.fillRect(x, flip ? py : py + h - 1.2, self.T, 1.2)
        }
        pad(y + max(-1.5, d), false)
        if ty < level.rows - 1 && level.tiles[i + level.cols] == 0 { pad(y + T - 5 - max(-1.5, d) * 0.6, true) }
    }

    func drawCrumble(_ g: Canvas2D, _ cur: EState, _ i: Int, _ x: Double, _ y: Double, _ t: Double, _ opts: DrawOpts) {
        let p = pal
        let c = crumbleIndex[i].flatMap { at(cur.crumble, $0) } ?? 0
        let sand = p.dark ? mixRgb(SAND, p.wall, 0.45) : mixRgb(SAND, p.wall, 0.3)
        if c > Ember.CRUMBLE_TICKS {
            let back = clamp((c - Ember.CRUMBLE_TICKS) / Ember.CRUMBLE_GONE_TICKS, 0, 1)
            g.stroke(css(sand, 0.6))
            g.lineWidth = 1
            g.setLineDash([1.5, 3])
            rr(g, x + 1.5, y + 1.5, T - 3, T - 3, 3)
            g.stroke()
            g.setLineDash([])
            let s = (T - 6) * back
            g.fill(css(sand, 0.1 + 0.16 * back))
            rr(g, x + T / 2 - s / 2, y + T / 2 - s / 2, s, s, 2)
            g.fill()
            return
        }
        let shake = c > 0 ? c / Ember.CRUMBLE_TICKS : 0
        let jx = shake != 0 && !opts.reduceMotion ? sin(t * 70 + Double(i)) * (0.4 + shake * 1.1) : 0
        let jy = shake != 0 && !opts.reduceMotion ? cos(t * 57 + Double(i * 3)) * shake * 0.7 : 0
        var k = 1.0
        if let at = mendAt[i] {
            let e = (t - at) / 0.22
            if e >= 1 { mendAt[i] = nil } else { k = 0.55 + 0.45 * e * (2 - e) }
        }
        g.save()
        g.translate(x + T / 2 + jx, y + T / 2 + jy)
        g.scale(k, k)
        g.translate(-T / 2, -T / 2)
        g.globalAlpha = k < 1 ? k : 1
        g.fill(css(BLACK, p.dark ? 0.3 : 0.1))
        rr(g, 1, 2, T, T, 3)
        g.fill()
        g.fill(css(sand))
        rr(g, 0, 0, T, T, 3)
        g.fill()
        g.fill(css(mixRgb(sand, WHITE, p.dark ? 0.3 : 0.5)))
        g.fillRect(2, 0, T - 4, 2.2)
        g.fill(css(mixRgb(sand, BLACK, 0.35), 0.7))
        g.fillRect(2, T - 2.2, T - 4, 2.2)
        let h = hash01(Double(i))
        g.stroke(css(mixRgb(sand, BLACK, 0.55), 0.85))
        g.lineWidth = 0.9 + shake * 0.7
        g.lineJoin = .round
        g.beginPath()
        g.moveTo(4 + h * 5, 2)
        g.lineTo(8 + h * 3, 7)
        g.lineTo(6 + h * 4, 11)
        g.moveTo(15 - h * 3, T - 2)
        g.lineTo(13 - h * 2, 13)
        if shake > 0.01 {
            g.lineTo(10 + h * 2, 10)
            g.lineTo(6 + h * 4, 11)
            g.moveTo(8 + h * 3, 7)
            g.lineTo(14, 5 + h * 2)
            if shake > 0.5 {
                g.lineTo(T - 2, 7)
                g.moveTo(6 + h * 4, 11)
                g.lineTo(3, 15)
                g.lineTo(5, T - 2)
            }
        }
        g.stroke()
        g.restore()
        g.globalAlpha = 1
        if shake > 0 && rnd() < (opts.reduceMotion ? 0.06 : 0.3) { parts.spawn(PK_DOT, C_SAND, x + rand(2, T - 2), y + T - 1, rand(-8, 8), rand(10, 40), 0.4, rand(0.9, 1.5), 300, 0.5) }
    }

    // ---- version 2's things

    func drawCheckpoints(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ t: Double, _ dt: Double) {
        let p = pal
        for (i, c) in level.checkpoints.enumerated() {
            let taken = i == cur.cp
            if cpUp.indices.contains(i) { cpUp[i] = approach(cpUp[i], taken ? 1 : 0, 9, dt) }
            if !vis(c.x - 14, c.y - 30, 30, 32) { continue }
            let up = at(cpUp, i) ?? 0
            let top = c.y - 27
            let e = t - (at(cpAt, i) ?? -9)
            let pop = e >= 0 && e < 0.7 ? exp(-e * 6) * sin(e * 20) : 0
            if up > 0.02 {
                beginGlow(g)
                glow(g, c.x + 5, top + 6, 20 + pop * 6, css(p.star), (0.5 + 0.08 * sin(t * 3)) * up)
                endGlow(g)
            }
            g.fill(css(p.dark ? mixRgb(METAL, BLACK, 0.3) : METAL))
            g.beginPath()
            g.ellipse(c.x, c.y, 5, 2.2, 0, .pi, 0)
            g.fill()
            g.fillRect(c.x - 0.9, top, 1.8, 27)
            g.fill(css(up > 0.5 ? p.star : p.muted))
            g.beginPath()
            g.arc(c.x, top - 0.5, 1.9, 0, TAU)
            g.fill()
            let len = 5 + 9 * up + pop * 4
            let hy = top + 2 + (1 - up) * 3
            let hgt = 9.0
            let wave = { (u: Double) in sin(t * 5.5 - u * 3.2 + Double(i)) * 1.5 * u * up }
            let droop = { (u: Double) in (1 - up) * u * 7 }
            for half in 0..<2 {
                let hf = Double(half)
                let y0 = hy + hf * hgt / 2
                let y1 = y0 + hgt / 2
                g.fill(up > 0.5 ? css(half != 0 ? p.frost : p.fire) : css(p.muted, 0.75))
                g.beginPath()
                g.moveTo(c.x + 0.9, y0)
                var u = 0.25
                while u <= 1.001 {
                    g.lineTo(c.x + 0.9 + len * u, y0 + (hgt / 2 - hf * (hgt / 2)) * u * (half != 0 ? 0 : 1) + wave(u) + droop(u))
                    u += 0.25
                }
                u = 1
                while u >= -0.001 {
                    g.lineTo(c.x + 0.9 + len * u, y1 - (half != 0 ? (hgt / 2) * u : 0) + wave(u) + droop(u))
                    u -= 0.25
                }
                g.closePath()
                g.fill()
            }
            if !taken && cur.phase == "play" {
                for k in 0..<2 {
                    let pl = cur.players[k]
                    let z = c.zone
                    let inside = pl.alive && pl.x < z.x + z.w && pl.x + Ember.PLAYER_W > z.x && pl.y < z.y + z.h && pl.y + Ember.PLAYER_H > z.y
                    g.fill(inside ? css(k == 0 ? p.fire : p.frost) : css(p.ink, 0.22))
                    g.beginPath()
                    g.arc(c.x + (k != 0 ? 4.2 : -4.2), c.y - 4.5, inside ? 1.9 : 1.4, 0, TAU)
                    g.fill()
                }
            }
        }
    }

    func drawLamps(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ t: Double, _ dt: Double) {
        let p = pal
        for (i, l) in level.lamps.enumerated() {
            if lampOn.indices.contains(i) { lampOn[i] = approach(lampOn[i], cur.lit ? 1 : 0, 18, dt) }
            if !vis(l.x, l.y, l.w, l.h, 30) { continue }
            let on = at(lampOn, i) ?? 0
            let cx = l.x + l.w / 2
            let by = l.y + l.h
            let hy = l.y + 6.5
            let metal = p.dark ? mixRgb(METAL, BLACK, 0.3) : METAL
            beginGlow(g)
            glow(g, cx, hy, 13 + on * 13, css(SUN), 0.22 + on * 0.5 + 0.04 * sin(t * 5 + Double(i)))
            endGlow(g)
            g.fill(css(metal))
            g.fillRect(cx - 1, hy + 4, 2, by - hy - 4)
            g.beginPath()
            g.ellipse(cx, by, 5, 2.2, 0, .pi, 0)
            g.fill()
            rr(g, cx - 4, hy + 3.4, 8, 2.2, 1)
            g.fill()
            g.fill(css(mixRgb(mixRgb(SUN, BLACK, 0.72), mixRgb(SUN, WHITE, 0.55), on)))
            g.beginPath()
            g.arc(cx, hy, 4.6, 0, TAU)
            g.fill()
            g.fill(css(mixRgb(LAVA_MID, WHITE, on), 0.9))
            g.beginPath()
            g.arc(cx, hy + 0.4, 1.4 + on * 0.8, 0, TAU)
            g.fill()
            g.stroke(css(metal))
            g.lineWidth = 0.9
            g.beginPath()
            g.arc(cx, hy, 4.9, 0, TAU)
            g.moveTo(cx, hy - 4.9)
            g.lineTo(cx, hy + 4.9)
            g.stroke()
            g.stroke(css(p.ink, 0.4))
            g.lineWidth = 0.7
            g.beginPath()
            g.moveTo(cx + 5.2, hy + 3)
            g.lineTo(cx + 5.2, hy + 8 + on)
            g.stroke()
            g.fill(css(SUN))
            g.beginPath()
            g.arc(cx + 5.2, hy + 8.6 + on, 1.1, 0, TAU)
            g.fill()
            if l.ticks > 0 {
                let frac = cur.litT > 0 ? clamp(cur.litT / l.ticks, 0, 1) : 0
                g.lineCap = .round
                g.stroke(css(p.ink, 0.18))
                g.lineWidth = 1.5
                g.setLineDash([1.2, 2.4])
                g.beginPath()
                g.arc(cx, hy, 7.6, 0, TAU)
                g.stroke()
                g.setLineDash([])
                if frac > 0 {
                    g.stroke(css(frac < 0.25 && sin(t * 20) > 0 ? WHITE : SUN))
                    g.lineWidth = 1.9
                    g.beginPath()
                    g.arc(cx, hy, 7.6, -.pi / 2, -.pi / 2 + TAU * frac)
                    g.stroke()
                }
            }
        }
    }

    func drawGswitches(_ g: Canvas2D, _ level: EStage, _ t: Double, _ dt: Double) {
        let p = pal
        for (i, s) in level.gswitches.enumerated() {
            if gswSpin.indices.contains(i) { gswSpin[i] = max(0, gswSpin[i] - dt * 2.4) }
            if !vis(s.x, s.y, s.w, s.h, 20) { continue }
            let who = s.who
            let spin = at(gswSpin, i) ?? 0
            let cx = s.x + s.w / 2
            let cy = s.y + 8.5 + sin(t * 2.2 + Double(i)) * 0.7
            let col = p.dark ? GRAV : mixRgb(GRAV, BLACK, 0.15)
            beginGlow(g)
            glow(g, cx, cy, 15 + spin * 8, css(GRAV), 0.4 + spin * 0.4)
            endGlow(g)
            g.fill(css(p.dark ? mixRgb(GRAV, BLACK, 0.74) : mixRgb(GRAV, WHITE, 0.72)))
            g.beginPath()
            g.arc(cx, cy, 7.2, 0, TAU)
            g.fill()
            g.lineWidth = 1.5
            if who == "both" {
                g.stroke(css(p.fire))
                g.beginPath()
                g.arc(cx, cy, 7.2, .pi * 0.5, .pi * 1.5)
                g.stroke()
                g.stroke(css(p.frost))
                g.beginPath()
                g.arc(cx, cy, 7.2, .pi * 1.5, .pi * 2.5)
                g.stroke()
            } else {
                g.stroke(css(who == "fire" ? p.fire : who == "frost" ? p.frost : col))
                if who == "other" {
                    g.setLineDash([2.6, 2.2])
                    g.lineDashOffset = -t * 6
                }
                g.beginPath()
                g.arc(cx, cy, 7.2, 0, TAU)
                g.stroke()
                g.setLineDash([])
            }
            g.save()
            g.translate(cx, cy)
            g.rotate(spin * spin * .pi)
            g.stroke(css(col))
            g.lineWidth = 1.4
            g.lineCap = .round
            g.lineJoin = .round
            g.beginPath()
            g.moveTo(-2.4, 3.6)
            g.lineTo(-2.4, -3.4)
            g.moveTo(-4.4, -1.4)
            g.lineTo(-2.4, -3.6)
            g.lineTo(-0.4, -1.4)
            g.moveTo(2.4, -3.6)
            g.lineTo(2.4, 3.4)
            g.moveTo(0.4, 1.4)
            g.lineTo(2.4, 3.6)
            g.lineTo(4.4, 1.4)
            g.stroke()
            g.restore()
            let ly = s.y + s.h - 2.6
            if who == "fire" || who == "both" {
                g.fill(css(p.fire))
                flameGlyph(g, cx - (who == "both" ? 3.4 : 0), ly - 0.4, 5.6)
                g.fill()
            }
            if who == "frost" || who == "both" {
                g.stroke(css(p.frost))
                g.lineWidth = 0.8
                flakeGlyph(g, cx + (who == "both" ? 3.4 : 0), ly - 0.4, 2.5)
                g.stroke()
            }
            if who == "self" || who == "other" {
                g.fill(css(p.ink, 0.75))
                g.stroke(css(p.ink, 0.75))
                g.lineWidth = 0.8
                g.beginPath()
                g.arc(cx - (who == "other" ? 4.4 : 0), ly, 1.5, 0, TAU)
                g.fill()
                if who == "other" {
                    g.beginPath()
                    g.moveTo(cx - 1.6, ly)
                    g.lineTo(cx + 1.6, ly)
                    g.moveTo(cx + 0.4, ly - 1.2)
                    g.lineTo(cx + 1.6, ly)
                    g.lineTo(cx + 0.4, ly + 1.2)
                    g.stroke()
                    g.beginPath()
                    g.arc(cx + 4.4, ly, 1.5, 0, TAU)
                    g.stroke()
                }
            }
        }
    }

    func drawLocks(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ t: Double, _ dt: Double) {
        let p = pal
        for (i, r) in level.locks.enumerated() {
            let open = at(cur.locks, i) ?? false
            if lockGone.indices.contains(i) { lockGone[i] = approach(lockGone[i], open ? 1 : 0, 11, dt) }
            let gone = at(lockGone, i) ?? 0
            if gone > 0.98 || !vis(r.x, r.y, r.w, r.h, 12) { continue }
            let cx = r.x + r.w / 2, cy = r.y + r.h / 2
            let body = p.dark ? mixRgb(METAL, BLACK, 0.5) : mixRgb(METAL, BLACK, 0.28)
            g.save()
            g.translate(cx, cy)
            g.scale(1 + gone * 0.18, 1 + gone * 0.18)
            g.translate(-cx, -cy)
            g.globalAlpha = 1 - gone
            g.fill(css(BLACK, p.dark ? 0.3 : 0.1))
            rr(g, r.x + 1, r.y + 2, r.w, r.h, 3)
            g.fill()
            g.fill(css(body))
            rr(g, r.x, r.y, r.w, r.h, 3)
            g.fill()
            g.fill(css(GOLD, 0.9))
            var yy = r.y + 3
            while yy < r.y + r.h - 3 {
                g.fillRect(r.x + 1, yy, r.w - 2, 1.6)
                yy += T
            }
            g.stroke(css(GOLD))
            g.lineWidth = 1.4
            rr(g, r.x + 0.7, r.y + 0.7, r.w - 1.4, r.h - 1.4, 3)
            g.stroke()
            let ready = cur.keys > 0 && !open
            if ready {
                beginGlow(g)
                glow(g, cx, cy, 14, css(GOLD), 0.5 + 0.25 * sin(t * 6))
                endGlow(g)
                g.globalAlpha = 1 - gone
            }
            g.fill(css(GOLD))
            g.beginPath()
            g.arc(cx, cy, 5.6, 0, TAU)
            g.fill()
            g.fill(css(ready ? mixRgb(GOLD_DARK, BLACK, 0.5) : mixRgb(body, BLACK, 0.5)))
            g.beginPath()
            g.arc(cx, cy - 1.2, 1.9, 0, TAU)
            g.fill()
            g.beginPath()
            g.moveTo(cx - 1, cy - 0.6)
            g.lineTo(cx + 1, cy - 0.6)
            g.lineTo(cx + 1.7, cy + 3.6)
            g.lineTo(cx - 1.7, cy + 3.6)
            g.closePath()
            g.fill()
            g.restore()
            g.globalAlpha = 1
        }
    }

    func keyAt(_ g: Canvas2D, _ x: Double, _ y: Double, _ s: Double, _ ang: Double) {
        g.save()
        g.translate(x, y)
        g.rotate(ang)
        g.lineCap = .round
        g.lineJoin = .round
        keyPath(g, s)
        g.stroke(css(GOLD_DARK))
        g.lineWidth = 3.2 * s
        g.stroke()
        g.stroke(css(GOLD))
        g.lineWidth = 1.7 * s
        g.stroke()
        g.fill(css(WHITE, 0.8))
        g.fillRect(-4.6 * s, -2 * s, 1.1 * s, 1.1 * s)
        g.restore()
    }

    func drawKeys(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ t: Double, _ opts: DrawOpts) {
        for (i, k) in level.keys.enumerated() {
            if at(cur.keyTaken, i) == true { continue }
            if !vis(k.x - 10, k.y - 10, 20, 20) { continue }
            let y = k.y + sin(t * 2.4 + Double(i) * 2.1) * 1.8
            beginGlow(g)
            glow(g, k.x, y, 17, css(GOLD), 0.5 + 0.12 * sin(t * 4 + Double(i)))
            endGlow(g)
            keyAt(g, k.x - 1, y, 1.1, -0.5 + sin(t * 1.7 + Double(i)) * 0.12)
            if rnd() < (opts.reduceMotion ? 0.01 : 0.035) { parts.spawn(PK_STAR, C_GOLD, k.x + rand(-7, 7), y + rand(-6, 6), 0, -4, 0.5, rand(2, 3), 0, 0, true) }
        }
    }

    func drawCarried(_ g: Canvas2D, _ cur: EState, _ t: Double, _ dt: Double) {
        let n = min(6, max(0, Int(cur.keys)))
        if keyTrail.count > n { keyTrail.removeLast(keyTrail.count - n) }
        if n == 0 { return }
        let h = cur.players[keyHolder].alive ? keyHolder : keyHolder == 0 ? 1 : 0
        let pl = cur.players[h]
        let gv: Double = pl.grav == -1 ? -1 : 1
        var tx = px[h] + Ember.PLAYER_W / 2 - pl.face * 13
        var ty = py[h] + Ember.PLAYER_H / 2 - gv * 7
        for i in 0..<n {
            if i >= keyTrail.count { keyTrail.append(EPoint(x: tx, y: ty)) }
            var k = keyTrail[i]
            k.x = approach(k.x, tx, 11 - Double(i) * 1.2, dt)
            k.y = approach(k.y, ty, 11 - Double(i) * 1.2, dt)
            if abs(k.x - tx) + abs(k.y - ty) > 160 {
                k.x = tx
                k.y = ty
            }
            keyTrail[i] = k
            let y = k.y + sin(t * 4 + Double(i) * 1.3) * 1.4
            if vis(k.x - 8, y - 8, 16, 16) {
                beginGlow(g)
                glow(g, k.x, y, 11, css(GOLD), 0.45)
                endGlow(g)
                keyAt(g, k.x, y, 0.8, -0.5 + sin(t * 3 + Double(i)) * 0.2)
            }
            tx = k.x - pl.face * 9
            ty = k.y
        }
    }

    func drawArrows(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ t: Double, _ opts: DrawOpts) {
        let p = pal
        let col = p.dark ? ARROW : mixRgb(ARROW, BLACK, 0.2)
        for (i, a) in level.arrows.enumerated() {
            if !vis(a.x - 10, a.y - 12, 20, 24) { continue }
            let left = at(cur.arrows, i) ?? 0
            g.lineCap = .round
            g.lineJoin = .round
            if left > 0 {
                let back = 1 - clamp(left / Ember.ARROW_RESPAWN_TICKS, 0, 1)
                g.save()
                g.translate(a.x, a.y)
                g.globalAlpha = 0.3 + 0.25 * back
                g.stroke(css(col))
                g.lineWidth = 1.1
                g.setLineDash([2, 2])
                chevrons(g, 1.25)
                g.stroke()
                g.setLineDash([])
                g.globalAlpha = 0.75
                g.lineWidth = 1.4
                g.beginPath()
                g.arc(0, 0, 8.6, -.pi / 2, -.pi / 2 + TAU * back)
                g.stroke()
                g.restore()
                g.globalAlpha = 1
                continue
            }
            let bob = opts.reduceMotion ? 0 : sin(t * 3 + Double(i) * 1.9) * 2.2
            let y = a.y + bob
            beginGlow(g)
            glow(g, a.x, y, 18, css(ARROW), 0.5 + 0.12 * sin(t * 5 + Double(i)))
            endGlow(g)
            g.save()
            g.translate(a.x, y)
            g.fill(css(p.dark ? mixRgb(ARROW, BLACK, 0.72) : mixRgb(ARROW, WHITE, 0.78), 0.85))
            g.beginPath()
            g.arc(0, 0, 8, 0, TAU)
            g.fill()
            g.stroke(css(col, 0.9))
            g.lineWidth = 1
            g.stroke()
            chevrons(g, 1.25)
            g.stroke(css(mixRgb(ARROW, BLACK, 0.5)))
            g.lineWidth = 3.2
            g.stroke()
            g.stroke(css(p.dark ? mixRgb(ARROW, WHITE, 0.45) : WHITE))
            g.lineWidth = 1.7
            g.stroke()
            g.restore()
            if rnd() < (opts.reduceMotion ? 0.01 : 0.05) { parts.spawn(PK_CHEV, C_ARROW, a.x + rand(-6, 6), y + 8, 0, -rand(20, 40), 0.5, 1.8, 0, 0, true) }
        }
    }

    func drawCannons(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ t: Double, _ dt: Double, _ opts: DrawOpts) {
        let p = pal
        for (i, c) in level.cannons.enumerated() {
            if cannonKick.indices.contains(i) { cannonKick[i] = max(0, cannonKick[i] - dt * 4) }
            if !vis(c.x - 12, c.y - 12, 24, 24, 220) { continue }
            let kick = at(cannonKick, i) ?? 0
            let ticks = at(cur.cannons, i) ?? 0
            let who = cur.players[0].cannon == i ? 0 : cur.players[1].cannon == i ? 1 : -1
            let loaded = ticks > 0 || who >= 0
            let ang = atan2(c.vy, c.vx)
            let chs = condChannels(c.fire)
            let ready = c.fire != nil && condHolds(c.fire, cur.on, cur.lit)
            let chCol = chs.first.map(channelColor) ?? css(p.muted)
            let gv: Double = who >= 0 && cur.players[who].grav == -1 ? -1 : 1
            g.fill(css(who == 0 ? p.fire : who == 1 ? p.frost : p.ink, loaded ? 0.7 : p.dark ? 0.26 : 0.22))
            let flyS = Ember.CANNON_FLY_TICKS / 60
            for k in 1...26 {
                let tt = Double(k) * 0.055 + (opts.reduceMotion || !loaded ? 0 : (t * 0.5).truncatingRemainder(dividingBy: 0.055))
                if tt > flyS { break }
                let x = c.x + c.vx * tt
                let y = c.y + c.vy * tt + 0.5 * Ember.GRAVITY * gv * tt * tt
                if world.tileAt(level, Int(floor(x / T)), Int(floor(y / T))) == Ember.T_WALL { break }
                if k < 3 { continue }
                g.beginPath()
                g.arc(x, y, loaded ? 1.5 : 1.1, 0, TAU)
                g.fill()
            }
            let metal = p.dark ? mixRgb(METAL, BLACK, 0.42) : mixRgb(METAL, BLACK, 0.22)
            let swell = loaded && !opts.reduceMotion ? 1 + 0.05 * sin(t * 22) : 1
            g.save()
            g.translate(c.x, c.y)
            g.rotate(ang)
            g.translate(-kick * kick * 5, 0)
            g.scale(1, swell)
            g.fill(css(BLACK, p.dark ? 0.3 : 0.12))
            rr(g, -8, -5.5, 21, 14, 5)
            g.fill()
            g.fill(css(metal))
            rr(g, -9, -7, 21, 14, 5)
            g.fill()
            g.fill(css(mixRgb(metal, WHITE, p.dark ? 0.22 : 0.4)))
            g.fillRect(-5, -7, 14, 2)
            g.fill(chCol)
            g.fillRect(-2.5, -7, 3, 14)
            g.fill(css(mixRgb(metal, WHITE, 0.35)))
            rr(g, 9.5, -8, 3.6, 16, 1.6)
            g.fill()
            g.fill(css(BLACK, 0.7))
            g.beginPath()
            g.ellipse(13, 0, 1.6, 5.4, 0, 0, TAU)
            g.fill()
            if who >= 0 {
                g.fill(css(who == 0 ? p.fire : p.frost2))
                g.beginPath()
                g.arc(12.2, 0, 4.6, -.pi / 2, .pi / 2)
                g.fill()
                g.fill(who == 0 ? Col(64, 22, 6) : Col(14, 48, 84))
                g.fillRect(14.4, -2.4, 1.3, 1.6)
                g.fillRect(14.4, 0.9, 1.3, 1.6)
            }
            g.restore()
            g.fill(css(mixRgb(metal, BLACK, 0.3)))
            g.beginPath()
            g.arc(c.x, c.y, 4.6, 0, TAU)
            g.fill()
            g.fill(loaded && (c.fire == nil || ready || sin(t * 10) > 0) ? chCol : css(mixRgb(metal, WHITE, 0.3)))
            g.beginPath()
            g.arc(c.x, c.y, 1.9, 0, TAU)
            g.fill()
            if loaded {
                beginGlow(g)
                glow(g, c.x, c.y, 18, css(who == 0 ? p.fire : p.frost), 0.45)
                endGlow(g)
                g.lineCap = .round
                if c.fire == nil {
                    let frac = clamp(ticks / Ember.CANNON_FUSE_TICKS, 0, 1)
                    g.stroke(css(p.ink, 0.2))
                    g.lineWidth = 1.6
                    g.beginPath()
                    g.arc(c.x, c.y, 12.5, 0, TAU)
                    g.stroke()
                    g.stroke(css(LAVA_TOP))
                    g.lineWidth = 2
                    g.beginPath()
                    g.arc(c.x, c.y, 12.5, -.pi / 2, -.pi / 2 + TAU * frac)
                    g.stroke()
                    let fa = -Double.pi / 2 + TAU * frac
                    if rnd() < (opts.reduceMotion ? 0.1 : 0.6) { parts.spawn(PK_SPARK, rnd() < 0.5 ? C_CORE : C_FIRE2, c.x + cos(fa) * 12.5, c.y + sin(fa) * 12.5, rand(-30, 30), rand(-40, 10), rand(0.2, 0.4), rand(0.7, 1.2), 120, 1, true) }
                } else {
                    g.stroke(chCol)
                    g.globalAlpha = 0.45 + 0.4 * abs(sin(t * 5))
                    g.lineWidth = 1.6
                    g.setLineDash([3, 3])
                    g.lineDashOffset = -t * 10
                    g.beginPath()
                    g.arc(c.x, c.y, 12.5, 0, TAU)
                    g.stroke()
                    g.setLineDash([])
                    g.globalAlpha = 1
                }
            }
        }
    }

    // ---- fans

    func drawFanColumns(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ dt: Double, _ opts: DrawOpts) {
        let p = pal
        for (i, f) in level.fans.enumerated() {
            let on = condHolds(f.when, cur.on, cur.lit)
            guard fanSpin.indices.contains(i) else { continue }
            fanSpin[i] = approach(fanSpin[i], on ? 1 : 0, on ? 3 : 1.5, dt)
            fanAng[i] += fanSpin[i] * dt * 22
            let spin = fanSpin[i]
            if !vis(f.x, f.y, f.w, f.h, 4) { continue }
            let ch = condChannels(f.when).first
            let fy = f.y + f.h - T
            if spin > 0.05 {
                let gr = CanvasGradient(.linear(CGPoint(x: 0, y: fy), CGPoint(x: 0, y: f.y)))
                gr.addColorStop(0, css(p.dark ? WHITE : p.frost, 0.09 * spin))
                gr.addColorStop(1, css(p.dark ? WHITE : p.frost, 0))
                g.fill(gr)
                g.fillRect(f.x + 2, f.y, f.w - 4, f.h - T)
                let rate = (f.w / T) * (opts.reduceMotion ? 3 : 10) * spin
                if rnd() < rate * dt { parts.spawn(PK_STREAK, C_WHITE, f.x + rand(3, f.w - 3), fy, 0, -rand(160, 260), (f.h - T) / 220, rand(5, 10), 0, 0, true) }
            }
            g.fill(css(p.dark ? mixRgb(METAL, BLACK, 0.45) : mixRgb(METAL, BLACK, 0.1)))
            rr(g, f.x, fy, f.w, T, 3)
            g.fill()
            var tx = f.x
            while tx < f.x + f.w {
                let cx = tx + T / 2, cy = fy + T / 2 + 1
                g.fill(css(BLACK, 0.4))
                g.beginPath()
                g.arc(cx, cy, 7.5, 0, TAU)
                g.fill()
                g.fill(css(p.dark ? mixRgb(METAL, WHITE, 0.25) : mixRgb(METAL, WHITE, 0.35)))
                for b in 0..<3 {
                    let ang = fanAng[i] + Double(b) * TAU / 3
                    g.beginPath()
                    g.ellipse(cx + cos(ang) * 3.6, cy + sin(ang) * 3.6, 3.6, 1.6, ang, 0, TAU)
                    g.fill()
                }
                g.fill(ch.map(channelColor) ?? css(p.muted))
                g.beginPath()
                g.arc(cx, cy, 1.6, 0, TAU)
                g.fill()
                tx += T
            }
            g.fill(css(p.dark ? mixRgb(METAL, WHITE, 0.1) : METAL))
            g.fillRect(f.x + 1, fy, f.w - 2, 2.2)
            if let ch {
                g.fill(channelColor(ch))
                g.fillRect(f.x + 1, fy + T - 2.5, f.w - 2, 1.6)
            }
        }
    }

    // ---- gems, beams

    func drawGems(_ g: Canvas2D, _ level: EStage, _ cur: EState, _ t: Double, _ opts: DrawOpts) {
        let p = pal
        for (i, gm) in level.gems.enumerated() {
            if at(cur.taken, i) == true { continue }
            if !vis(gm.x - 8, gm.y - 8, 16, 16) { continue }
            let fire = gm.el == "fire"
            let c = fire ? p.fire : p.frost
            let c2 = fire ? p.fire2 : p.frost2
            let y = gm.y + sin(t * 2.6 + Double(i) * 1.7) * 1.6
            beginGlow(g)
            glow(g, gm.x, y, 16, css(c), 0.5 + 0.12 * sin(t * 4 + Double(i)))
            endGlow(g)
            g.save()
            g.translate(gm.x, y)
            let turn = 0.75 + 0.25 * cos(t * 1.8 + Double(i))
            g.scale(turn, 1)
            gemPath(g, 1)
            g.fill(css(c))
            g.fill()
            g.fill(css(c2))
            g.beginPath()
            g.moveTo(-5, -1.5)
            g.lineTo(-2.8, -4.5)
            g.lineTo(2.8, -4.5)
            g.lineTo(5, -1.5)
            g.closePath()
            g.fill()
            g.fill(css(mixRgb(c, BLACK, 0.25)))
            g.beginPath()
            g.moveTo(1.5, -1.5)
            g.lineTo(5, -1.5)
            g.lineTo(0, 5.5)
            g.closePath()
            g.fill()
            g.fill(Col(255, 255, 255, 0.85))
            g.beginPath()
            g.moveTo(-2.6, -3.9)
            g.lineTo(-0.6, -3.9)
            g.lineTo(-2.4, -1.9)
            g.closePath()
            g.fill()
            g.stroke(css(mixRgb(c, BLACK, 0.4), 0.7))
            g.lineWidth = 0.6
            gemPath(g, 1)
            g.stroke()
            g.restore()
            if rnd() < (opts.reduceMotion ? 0.01 : 0.03) { parts.spawn(PK_STAR, fire ? C_FIRE2 : C_WHITE, gm.x + rand(-6, 6), y + rand(-6, 5), 0, -4, 0.5, rand(2, 3.2), 0, 0, true) }
        }
    }

    func drawBeams(_ g: Canvas2D, _ t: Double, _ opts: DrawOpts) {
        if beams.isEmpty { return }
        let p = pal
        let star = css(p.star)
        let fl = 0.9 + 0.1 * sin(t * 31)
        g.lineCap = .round
        beginGlow(g)
        for b in beams {
            g.stroke(star)
            g.globalAlpha = (p.dark ? 0.18 : 0.14) * fl
            g.lineWidth = 8
            g.beginPath()
            g.moveTo(b.0, b.1)
            g.lineTo(b.2, b.3)
            g.stroke()
            g.globalAlpha = (p.dark ? 0.45 : 0.35) * fl
            g.lineWidth = 3.4
            g.stroke()
        }
        endGlow(g)
        g.stroke(colors[C_CORE])
        g.lineWidth = 1.3
        g.beginPath()
        for b in beams {
            g.moveTo(b.0, b.1)
            g.lineTo(b.2, b.3)
        }
        g.stroke()
        beginGlow(g)
        for b in beams { glow(g, b.2, b.3, 9, star, 0.7 * fl) }
        endGlow(g)
        if rnd() < (opts.reduceMotion ? 0.05 : 0.25) {
            let b = beams[Int(floor(rnd() * Double(beams.count)))]
            parts.spawn(PK_STAR, C_STAR, b.2, b.3, rand(-20, 20), rand(-20, 20), 0.35, 2.4, 0, 2, true)
        }
    }
}
