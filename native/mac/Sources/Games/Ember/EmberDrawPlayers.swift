import AppKit
import Foundation

// emberDraw.ts, the two characters and the select screen's thumbnails. Line for line with the TS.

extension EmberRenderer {
    func drawPlayer(_ g: Canvas2D, _ i: Int, _ cur: EState, _ t: Double, _ dt: Double, _ opts: DrawOpts) {
        let pl = cur.players[i]
        let PW = Ember.PLAYER_W, PH = Ember.PLAYER_H
        let x = px[i], y = py[i]
        let gv: Double = pl.grav == -1 ? -1 : 1
        vyRel = pl.vy * gv
        animate(&anims[i], pl, t, dt)
        anims[i].flip = opts.reduceMotion ? (gv < 0 ? 1 : 0) : approach(anims[i].flip, gv < 0 ? 1 : 0, 16, dt)
        anims[i].aura = approach(anims[i].aura, gv < 0 ? 1 : 0, 8, dt)
        let fire = i == 0
        let cx = x + PW / 2
        let feet = gv > 0 ? y + PH : y
        anims[i].x = cx
        anims[i].y = feet
        if !pl.alive || pl.cannon >= 0 { return }
        if !vis(x, y, PW, PH, 40) { return }
        let an = anims[i]
        let p = pal
        let cy = y + PH / 2
        let moving = abs(pl.vx) > 30
        let bob = pl.ground && moving ? abs(sin(an.run)) * 1.6 : 0
        let breathe = 1 + sin(t * 3.1 + Double(i)) * 0.025
        let sy = (1 + an.sq) * breathe
        let sx = 1 - an.sq * 0.55
        beginGlow(g)
        glow(g, cx, cy, fire ? 30 : 26, css(fire ? p.fire : p.frost), fire ? 0.55 + 0.08 * sin(t * 13) : 0.42)
        if an.aura > 0.02 { glow(g, cx, cy, 30, css(GRAV), 0.5 * an.aura) }
        endGlow(g)
        if pl.ground {
            g.fill(css(BLACK, p.dark ? 0.3 : 0.12))
            g.beginPath()
            g.ellipse(cx, feet, 7 * sx, 1.6, 0, 0, TAU)
            g.fill()
        }
        if an.aura > 0.02 {
            g.save()
            g.globalAlpha = an.aura
            g.stroke(css(p.dark ? GRAV : mixRgb(GRAV, BLACK, 0.15), 0.9))
            g.lineWidth = 1.3
            g.setLineDash([4, 4])
            g.lineDashOffset = opts.reduceMotion ? 0 : -t * 14
            g.beginPath()
            g.ellipse(cx, cy, 13, 16, 0, 0, TAU)
            g.stroke()
            g.setLineDash([])
            g.lineCap = .round
            g.lineJoin = .round
            g.lineWidth = 1.6
            for s in [-1.0, 1.0] {
                let ay = cy + (opts.reduceMotion ? 0 : -(t * 14 + (s > 0 ? 8 : 0)).truncatingRemainder(dividingBy: 16) + 8)
                g.globalAlpha = an.aura * (1 - abs(ay - cy) / 10)
                g.beginPath()
                g.moveTo(cx + s * 13 - 2.6, ay + 1.6)
                g.lineTo(cx + s * 13, ay - 1.6)
                g.lineTo(cx + s * 13 + 2.6, ay + 1.6)
                g.stroke()
            }
            g.restore()
            g.globalAlpha = 1
        }
        if pl.fly > 0 {
            var sp = hypot(pl.vx, pl.vy)
            if sp == 0 { sp = 1 }
            g.fill(css(fire ? p.fire2 : p.frost2))
            for k in 1...4 {
                let kd = Double(k)
                g.globalAlpha = 0.34 - kd * 0.07
                g.beginPath()
                g.arc(cx - pl.vx / sp * kd * 7, cy - pl.vy / sp * kd * 7, 7.5 - kd * 1.2, 0, TAU)
                g.fill()
            }
            g.globalAlpha = 1
        }
        g.save()
        g.translate(cx, cy)
        if an.flip > 0.001 {
            g.rotate(.pi * an.flip)
            if an.flip > 0.5 { g.scale(-1, 1) }
        }
        g.translate(0, PH / 2 - bob)
        g.rotate(an.tilt)
        g.scale(sx, sy)
        let me = opts.me == i
        let happy = cur.phase == "won" || cur.phase == "clear" || pl.inDoor
        if fire { drawEmber(g, an, pl, t, me, happy) } else { drawFrost(g, an, pl, t, me, happy) }
        g.restore()

        if pl.air > 0 {
            let col = p.dark ? ARROW : mixRgb(ARROW, BLACK, 0.2)
            let pulse = opts.reduceMotion ? 0.5 : 0.5 + 0.5 * sin(t * 9)
            beginGlow(g)
            glow(g, cx, feet + gv * 4, 13, css(ARROW), 0.5 + 0.3 * pulse)
            endGlow(g)
            g.save()
            g.translate(cx, feet + gv * (5 + pulse * 1.5))
            g.scale(1, gv)
            g.lineCap = .round
            g.lineJoin = .round
            chevrons(g, 1.15)
            g.stroke(css(mixRgb(ARROW, BLACK, 0.55)))
            g.lineWidth = 3
            g.stroke()
            g.stroke(css(p.dark ? mixRgb(ARROW, WHITE, 0.4) : WHITE))
            g.lineWidth = 1.5
            g.stroke()
            g.restore()
            g.stroke(css(col, 0.55 + 0.3 * pulse))
            g.lineWidth = 1.1
            g.beginPath()
            g.ellipse(cx, cy, 11.5, 14.5, 0, 0, TAU)
            g.stroke()
        }

        let up = { (d: Double) in feet - gv * d }
        let rate = (pl.fly > 0 ? 70.0 : moving || !pl.ground ? 26 : 0) * (opts.reduceMotion ? 0.3 : 1)
        anims[i].trail += rate * dt
        anims[i].ambient += (fire ? 7 : 3) * (opts.reduceMotion ? 0.3 : 1) * dt
        while anims[i].trail >= 1 {
            anims[i].trail -= 1
            if fire {
                parts.spawn(PK_SPARK, rnd() < 0.5 ? C_FIRE2 : C_CORE, cx - pl.face * rand(2, 6), up(rand(6, 16)), -pl.vx * 0.15 + rand(-10, 10), gv * rand(-40, -10), rand(0.3, 0.55), rand(0.6, 1.1), gv * -40, 1, true)
            } else {
                parts.spawn(PK_FLAKE, C_FROST2, cx - pl.face * rand(2, 6), up(rand(4, 18)), -pl.vx * 0.12 + rand(-8, 8), gv * rand(-6, 12), rand(0.5, 0.9), rand(1.4, 2.2), gv * 10, 1.5, true, 3)
            }
        }
        while anims[i].ambient >= 1 {
            anims[i].ambient -= 1
            if fire {
                parts.spawn(PK_SPARK, C_FIRE2, cx + rand(-3, 3), up(22 + bob), rand(-8, 8), gv * rand(-45, -25), rand(0.35, 0.6), rand(0.5, 0.9), gv * -20, 1, true)
            } else {
                parts.spawn(PK_STAR, C_WHITE, cx + rand(-7, 7), up(rand(6, 24)), 0, gv * -3, 0.45, rand(1.8, 2.6), 0, 0, true)
            }
            if gv < 0 && !opts.reduceMotion && rnd() < 0.6 { parts.spawn(PK_CHEV, C_GRAV, cx + rand(-10, 10), cy + rand(-4, 12), 0, -rand(30, 60), 0.5, 2, 0, 0, true) }
        }

        if me && cur.phase == "play" && cur.tick < 150 {
            let fade = clamp((150 - cur.tick) / 30, 0, 1)
            let ay = y - 12 + sin(t * 6) * 1.5
            g.globalAlpha = fade
            g.fill(css(p.accent))
            g.beginPath()
            g.moveTo(cx - 4, ay - 4)
            g.lineTo(cx + 4, ay - 4)
            g.lineTo(cx, ay + 1)
            g.closePath()
            g.fill()
            g.font = .systemFont(ofSize: 8, weight: .semibold)
            g.textAlign = .center
            g.textBaseline = .alphabetic
            g.fillText("you", cx, ay - 6)
            g.globalAlpha = 1
        }
    }

    func animate(_ an: inout PAnim, _ pl: EPlayer, _ t: Double, _ dt: Double) {
        if an.wasGround && !pl.ground && vyRel < 0 { an.sqv += 5 }
        if !an.wasGround && pl.ground { an.sqv -= 6 }
        an.wasGround = pl.ground
        var left = dt
        while left > 0 {
            let h = min(left, 1.0 / 240)
            an.sqv += (-320 * an.sq - 16 * an.sqv) * h
            an.sq += an.sqv * h
            left -= h
        }
        an.sq = clamp(an.sq, -0.32, 0.32)
        if pl.ground { an.run += dt * abs(pl.vx) * 0.085 }
        an.tilt = approach(an.tilt, clamp(pl.vx / Ember.RUN_SPEED, -1, 1) * 0.12, 10, dt)
        an.look = approach(an.look, pl.face, 12, dt)
        if t > an.blinkAt {
            an.blinkUntil = t + 0.11
            an.blinkAt = t + (rnd() < 0.2 ? 0.25 : 2 + rnd() * 3)
        }
    }

    func eyes(_ g: Canvas2D, _ an: PAnim, _ t: Double, _ y: Double, _ ink: RGBA, _ happy: Bool) {
        let lx = an.look * 1.7
        let ly = vyRel < -60 ? -0.7 : vyRel > 200 ? 0.6 : 0
        let blink = t < an.blinkUntil
        for s in [-1.0, 1.0] {
            let ex = s * 2.9 + lx
            let ey = y + ly
            if happy {
                g.stroke(ink)
                g.lineWidth = 1.1
                g.lineCap = .round
                g.beginPath()
                g.arc(ex, ey + 0.8, 1.5, .pi * 1.15, .pi * 1.85)
                g.stroke()
                continue
            }
            g.fill(ink)
            g.beginPath()
            g.ellipse(ex, ey, 1.25, blink ? 0.3 : 2, 0, 0, TAU)
            g.fill()
            if !blink {
                g.fill(RGBA(255, 255, 255, 0.95))
                g.fillRect(ex - 0.2 + an.look * 0.3, ey - 1.3, 0.75, 0.75)
            }
        }
    }

    func drawEmber(_ g: Canvas2D, _ an: PAnim, _ pl: EPlayer, _ t: Double, _ me: Bool, _ happy: Bool) {
        let p = pal
        let fl = sin(t * 17) * 0.9 + sin(t * 11.3 + 1) * 0.6
        let lean = -an.tilt * 40 - clamp(vyRel / 400, -1, 1) * 0.5
        let tipX = lean + sin(t * 6.3) * 0.9
        let tipY = -25 - fl - max(0, an.sq) * 6
        let fire = css(p.fire)
        g.fill(fire)
        for s in [-1.0, 1.0] {
            let h = 7 + sin(t * 13 + s * 2) * 1.6
            let bx = s * 6
            g.beginPath()
            g.moveTo(bx - 2.6, -9)
            g.quadraticCurveTo(bx + s * 2.5, -11 - h * 0.4, bx + s * 2.2 + tipX * 0.2, -10 - h)
            g.quadraticCurveTo(bx + s * 0.5, -10, bx + 2.6, -8)
            g.closePath()
            g.fill()
        }
        flame(g, 8, -8, tipX, tipY)
        if me {
            g.stroke(css(p.accent, 0.7))
            g.lineWidth = 2.4
            g.stroke()
        }
        g.fill(fire)
        g.fill()
        flame(g, 5.8, -6.6, tipX * 0.75, -19 - sin(t * 19 + 2) * 1.2)
        g.fill(css(p.fire2))
        g.fill()
        flame(g, 3.4, -4.8, tipX * 0.5, -12.5 - sin(t * 23) * 0.8)
        g.fill(RGBA(255, 246, 214, 0.95))
        g.fill()
        eyes(g, an, t, -10.5, RGBA(64, 22, 6), happy)
        g.fill(RGBA(255, 90, 60, 0.45))
        g.beginPath()
        g.ellipse(-5 + an.look * 1.2, -7.6, 1.5, 0.9, 0, 0, TAU)
        g.ellipse(5 + an.look * 1.2, -7.6, 1.5, 0.9, 0, 0, TAU)
        g.fill()
        if happy || !pl.ground {
            g.stroke(RGBA(64, 22, 6))
            g.lineWidth = 0.9
            g.beginPath()
            if happy { g.arc(an.look * 1.7, -7.8, 1.6, 0.15 * .pi, 0.85 * .pi) } else { g.ellipse(an.look * 1.7, -6.8, 0.8, 1.1, 0, 0, TAU) }
            g.stroke()
        }
    }

    func flame(_ g: Canvas2D, _ r: Double, _ cy: Double, _ tipX: Double, _ tipY: Double) {
        g.beginPath()
        g.moveTo(r, cy)
        g.arc(0, cy, r, 0, .pi)
        g.bezierCurveTo(-r, cy - r * 0.95, tipX - r * 0.45, tipY + r * 1.1, tipX, tipY)
        g.bezierCurveTo(tipX + r * 0.45, tipY + r * 1.1, r, cy - r * 0.95, r, cy)
        g.closePath()
    }

    func drawFrost(_ g: Canvas2D, _ an: PAnim, _ pl: EPlayer, _ t: Double, _ me: Bool, _ happy: Bool) {
        let p = pal
        let edge = css(p.frost)
        let pale = mixRgb(p.frost2, WHITE, 0.55)
        let shimmer = (t * 0.6).truncatingRemainder(dividingBy: 3)
        let spikes: [(Double, Double, Double, Double)] = [(-5.5, -15, -0.5, 7.5), (5.5, -15, 0.5, 7.5), (0, -17, 0, 10 + max(0, an.sq) * 5)]
        for (k, sp) in spikes.enumerated() {
            let (bx, by, ang, len) = sp
            let tx = bx + sin(ang) * len - an.tilt * 8
            let ty = by - cos(ang) * len
            let w = 2.6
            let nx = cos(ang) * w, ny = sin(ang) * w
            g.fill(css(p.frost2))
            g.beginPath()
            g.moveTo(bx - nx, by - ny)
            g.lineTo(tx, ty)
            g.lineTo(bx + nx, by + ny)
            g.closePath()
            g.fill()
            g.fill(css(abs(shimmer - Double(k)) < 0.4 ? WHITE : pale, 0.9))
            g.beginPath()
            g.moveTo(bx - nx, by - ny)
            g.lineTo(tx, ty)
            g.lineTo(bx, by)
            g.closePath()
            g.fill()
        }
        g.beginPath()
        g.moveTo(-7.5, -4)
        g.lineTo(-8.2, -11.5)
        g.lineTo(-4, -18)
        g.lineTo(4, -18)
        g.lineTo(8.2, -11.5)
        g.lineTo(7.5, -4)
        g.quadraticCurveTo(6.5, 0, 0, 0)
        g.quadraticCurveTo(-6.5, 0, -7.5, -4)
        g.closePath()
        g.lineJoin = .round
        if me {
            g.stroke(css(p.accent, 0.7))
            g.lineWidth = 3.2
            g.stroke()
        }
        let body = CanvasGradient(.linear(CGPoint(x: -8, y: -18), CGPoint(x: 8, y: 0)))
        body.addColorStop(0, css(pale))
        body.addColorStop(1, css(p.frost2))
        g.fill(body)
        g.fill()
        g.stroke(edge)
        g.lineWidth = 1.1
        g.stroke()
        g.fill(RGBA(255, 255, 255, 0.6))
        g.beginPath()
        g.moveTo(-4, -18)
        g.lineTo(0.5, -18)
        g.lineTo(-2.5, -13.5)
        g.lineTo(-8.2, -11.5)
        g.closePath()
        g.fill()
        g.stroke(css(p.frost, 0.35))
        g.lineWidth = 0.6
        g.beginPath()
        g.moveTo(-2.5, -13.5)
        g.lineTo(2.5, -13.5)
        g.lineTo(8.2, -11.5)
        g.moveTo(2.5, -13.5)
        g.lineTo(4, -18)
        g.stroke()
        eyes(g, an, t, -9, RGBA(14, 48, 84), happy)
        g.fill(RGBA(120, 170, 255, 0.45))
        g.beginPath()
        g.ellipse(-5 + an.look * 1.2, -6.2, 1.4, 0.8, 0, 0, TAU)
        g.ellipse(5 + an.look * 1.2, -6.2, 1.4, 0.8, 0, 0, TAU)
        g.fill()
        if happy || !pl.ground {
            g.stroke(RGBA(14, 48, 84))
            g.lineWidth = 0.9
            g.beginPath()
            if happy { g.arc(an.look * 1.7, -6.2, 1.6, 0.15 * .pi, 0.85 * .pi) } else { g.ellipse(an.look * 1.7, -5.2, 0.8, 1.1, 0, 0, TAU) }
            g.stroke()
        }
    }
}

/// drawThumb: a small static picture of a stage for the select screen, the whole stage fitted in
func drawEmberThumb(_ g: Canvas2D, _ level: EStage, stages: Int, dark: Bool) {
    let p = readPalette(dark: dark)
    let TILE = Ember.TILE
    let cw = Double(g.width), ch = Double(g.height)
    let k = min(cw / level.w, ch / level.h)
    let ox = (cw - level.w * k) / 2, oy = (ch - level.h * k) / 2
    let u = max(1, level.w / 640, level.h / 360)
    g.resetTransform()
    g.clearRect(0, 0, cw, ch)
    g.fill(css(p.wall))
    g.fillRect(0, 0, cw, ch)
    g.setTransform(k, 0, 0, k, ox, oy)
    let sky = CanvasGradient(.linear(CGPoint(x: 0, y: 0), CGPoint(x: 0, y: level.h)))
    sky.addColorStop(0, css(mixRgb(p.bg, p.tape, 0.15)))
    sky.addColorStop(1, css(p.dark ? mixRgb(p.tape, BLACK, 0.12) : mixRgb(p.tape, p.wall, 0.15)))
    g.fill(sky)
    g.fillRect(0, 0, level.w, level.h)
    paintWalls(g, level, p, false, nil)
    for i in 0..<level.tiles.count {
        let t = level.tiles[i]
        if t == 0 || t == Ember.T_WALL { continue }
        let tx = i % level.cols, ty = (i - tx) / level.cols
        let x = Double(tx) * TILE, y = Double(ty) * TILE
        if t == Ember.T_LAVA || t == Ember.T_WATER || t == Ember.T_GOO || t == Ember.T_THIN {
            let top = ty > 0 && level.tiles[i - level.cols] == t ? 0 : Ember.POOL_SINK
            g.fill(css(t == Ember.T_LAVA ? LAVA_MID : t == Ember.T_WATER ? WATER_TOP : t == Ember.T_GOO ? GOO_TOP : THIN_TOP))
            g.fillRect(x, y + top, TILE + 0.5, TILE - top + 0.5)
        } else if t == Ember.T_TRAMP {
            g.fill(css(mixRgb(METAL, BLACK, 0.3)))
            g.fillRect(x, y + 5, TILE + 0.5, TILE - 5)
            g.fill(css(TRAMP))
            g.fillRect(x, y, TILE + 0.5, 6 * min(u, 2))
        } else if t == Ember.T_ONEWAY {
            g.fill(css(WOOD))
            g.fillRect(x, y, TILE + 0.5, Ember.ONEWAY_H * min(u, 2))
        } else if t == Ember.T_CRUMBLE {
            g.fill(css(mixRgb(SAND, p.wall, 0.3)))
            g.fillRect(x + 0.5, y + 0.5, TILE - 1, TILE - 1)
        } else {
            let sun = t == Ember.T_LIGHT
            let there = sun != level.dark
            g.fill(css(sun ? SUN : MOON, there ? 0.95 : 0.3))
            g.fillRect(x + 1, y + 1, TILE - 2, TILE - 2)
        }
    }
    let solid = { (x: Double, y: Double, w: Double, h: Double, c: RGB) in
        g.fill(css(c))
        rr(g, x, y, w, h, 2)
        g.fill()
    }
    let dot = { (x: Double, y: Double, r: Double, c: RGB) in
        g.fill(css(c))
        g.beginPath()
        g.arc(x, y, r, 0, TAU)
        g.fill()
    }
    for r in level.ice { solid(r.x, r.y, r.w, r.h, ICE_BOT) }
    for gt in level.gates { solid(gt.x, gt.y, gt.w, gt.h, p.dark ? mixRgb(METAL, BLACK, 0.2) : METAL) }
    for l in level.locks {
        solid(l.x, l.y, l.w, l.h, mixRgb(METAL, BLACK, 0.35))
        dot(l.x + l.w / 2, l.y + l.h / 2, 5 * min(u, 2), GOLD)
    }
    for m in level.movers { solid(m.x, m.y, m.w, m.h, mixRgb(p.wall, METAL, 0.5)) }
    for b in level.boxes { solid(b.x, b.y, TILE, TILE, WOOD) }
    for f in level.fans { solid(f.x, f.y + f.h - TILE, f.w, TILE, mixRgb(METAL, BLACK, 0.3)) }
    var pairs: [String] = []
    for pt in level.portals {
        var k2 = pairs.firstIndex(of: pt.pair) ?? -1
        if k2 < 0 {
            pairs.append(pt.pair)
            k2 = pairs.count - 1
        }
        g.fill(css(PORTAL_COLORS[k2 % PORTAL_COLORS.count]))
        g.beginPath()
        g.ellipse(pt.x + pt.w / 2, pt.y + pt.h / 2, pt.w / 2 - 1, pt.h / 2 - 1, 0, 0, TAU)
        g.fill()
    }
    for pl in level.plates { solid(pl.x, pl.ceil ? pl.y : pl.y + pl.h - 4, pl.w, 4, rgbOf(channelColor(pl.ch))) }
    for b in level.buttons { solid(b.x + 3, b.ceil ? b.y : b.y + b.h - 5, b.w - 6, 5, rgbOf(channelColor(b.ch))) }
    for c in level.cannons {
        var len = hypot(c.vx, c.vy)
        if len == 0 { len = 1 }
        dot(c.x, c.y, 8, mixRgb(METAL, BLACK, 0.3))
        dot(c.x + c.vx / len * 8, c.y + c.vy / len * 8, 5, mixRgb(METAL, BLACK, 0.3))
    }
    for s in level.gswitches { dot(s.x + s.w / 2, s.y + s.h / 2, 6 * min(u, 2), GRAV) }
    for l in level.lamps { dot(l.x + l.w / 2, l.y + l.h / 2, 6 * min(u, 2), SUN) }
    for a in level.arrows { dot(a.x, a.y, 5 * min(u, 2), ARROW) }
    for key in level.keys { dot(key.x, key.y, 5 * min(u, 2), GOLD) }
    for c in level.checkpoints {
        g.fill(css(p.star))
        g.fillRect(c.x - 1, c.y - 24, 2 * min(u, 2), 24)
        g.beginPath()
        g.moveTo(c.x, c.y - 24)
        g.lineTo(c.x + 12 * min(u, 2), c.y - 19)
        g.lineTo(c.x, c.y - 14)
        g.closePath()
        g.fill()
    }
    for (i, d) in level.doors.enumerated() {
        let c = i == 0 ? p.fire : p.frost
        let e = (min(u, 2.5) - 1) * 6
        archPath(g, d.x - 2 - e, d.y - 2 - 2 * e, d.w + 4 + 2 * e, d.h + 2 + 2 * e)
        g.fill(css(c))
        g.fill()
        archPath(g, d.x + 2, d.y + 2, d.w - 4, d.h - 2)
        g.fill(css(mixRgb(c, BLACK, p.dark ? 0.6 : 0.35)))
        g.fill()
    }
    for gm in level.gems {
        g.save()
        g.translate(gm.x, gm.y)
        g.scale(1.4 * min(u, 2.5), 1.4 * min(u, 2.5))
        gemPath(g, 1)
        g.fill(css(gm.el == "fire" ? p.fire : p.frost))
        g.fill()
        g.restore()
    }
    let hero = { (i: Int) in EPoint(x: level.spawns[i].x + Ember.PLAYER_W / 2, y: level.spawns[i].y + Ember.PLAYER_H - 9) }
    if level.dark {
        g.save()
        g.beginPath()
        if !level.darks.isEmpty { for r in level.darks { g.rect(r.x, r.y, r.w, r.h) } } else { g.rect(-ox / k, -oy / k, cw / k, ch / k) }
        g.clip()
        g.fill(RGBA(7, 6, 14, 0.8))
        g.fillRect(-ox / k, -oy / k, cw / k, ch / k)
        for i in 0..<2 {
            let h = hero(i)
            let r = (i == 0 ? 110.0 : 60) * min(u, 1.6)
            let gr = CanvasGradient(.radial(CGPoint(x: h.x, y: h.y), 0, CGPoint(x: h.x, y: h.y), r))
            gr.addColorStop(0, css(i == 0 ? p.fire : p.frost, 0.55))
            gr.addColorStop(1, css(i == 0 ? p.fire : p.frost, 0))
            g.fill(gr)
            g.fillRect(h.x - r, h.y - r, 2 * r, 2 * r)
        }
        g.restore()
    }
    for i in 0..<2 { dot(hero(i).x, hero(i).y, 8 * min(u, 2.5), i == 0 ? p.fire : p.frost) }
    g.resetTransform()
    let n = min(6, stages)
    if n > 1 {
        let r = max(2, ch * 0.035)
        let gap = r * 3.2
        let bw = gap * Double(n - 1) + r * 4
        let by = ch - r * 2.6
        g.fill(p.dark ? RGBA(0, 0, 0, 0.6) : RGBA(255, 255, 255, 0.78))
        rr(g, cw / 2 - bw / 2, by - r * 1.8, bw, r * 3.6, r * 1.8)
        g.fill()
        for i in 0..<n {
            g.fill(i == 0 ? css(p.ink) : css(p.ink, 0.32))
            g.beginPath()
            g.arc(cw / 2 - gap * Double(n - 1) / 2 + Double(i) * gap, by, i == 0 ? r : r * 0.8, 0, TAU)
            g.fill()
        }
    }
}
