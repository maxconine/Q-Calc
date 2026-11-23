#let choose = "choose"
#let multichoose = "multichoose"
#let multibinom(upper, ..lower) = {
  let helper = math.binom(upper, ..lower)
  $(helper)$
}
#let amat(..rows) = {
  math.mat(..rows, augment: -1)
}

#let emat(..rows) = {
  math.mat(..rows, delim: "(")
}

#let dmat(..rows) = {
  math.mat(..rows, delim: "|")
}

#let tan1 = $tan^(-1)$

#let dt = $space d t$
#let dtau = $space d tau$

#let bu = $bold(u)$
#let bv = $bold(v)$
#let bw = $bold(w)$
#let b0 = $bold(0)$
#let bx = $bold(x)$
#let bP = $bold(P)$
#let bF = $bold(F)$
#let bz = $bold(z)$
#let ba = $bold(a)$
#let bc = $bold(c)$

#let lp = $cal(L)$

#let pm = $plus.minus$

#let proj = $text("proj")$
#let span = $op("span")$
#let null = $op("null")$
#let row = $op("row")$
#let col = $op("col")$
#let rank = $op("rank")$
#let range = $op("range")$
#let nullity = $op("nullity")$
#let cov = $op("cov")$

#let sa = $stretch(->)$
#let ssa = $stretch(=>)$
#let se = $stretch(=)$

#let dd = math.dot.double

#let dx = $space d x$

#let ibar(content) = $lr(#content space |)$

#let tand = $space.quad text("and") space.quad$

#let imod(n) = $space (mod #n)$
#let ctimes = $times.o$
#let cplus = $plus.o$
#let cminus = $minus.o$

#let circ = math.op($circle.small$)

#let cC = $cal(C)$
#let cD = $cal(D)$
#let cB = $cal(B)$
#let cP = $cal(P)$

#let ip(x, y) = $lr(chevron.l #x, #y chevron.r)$
#let ict = math.op("ict")
#let cross = $times$

// native only: ans in the preview, the number it stands for boxed in blue with an "ans" pill on its edge, as desmos
// draws it; the number keeps the text colour, so it reads in light and dark, and sits on the line like the rest
// (the baseline drops by the pill room and the bottom padding, 6.5pt + 3pt; the room keeps a fraction bar clear)
#let ansblue = rgb("#3478f6")
#let anschip(body) = box(baseline: 9.5pt, inset: (bottom: 6.5pt), box(inset: (x: 2.5pt, top: 2pt, bottom: 3pt), radius: 3pt,
  stroke: 0.9pt + ansblue, fill: ansblue.transparentize(85%), {
    body
    place(bottom + center, dy: 7.5pt, box(inset: (x: 2pt, y: 1.2pt), radius: 2pt, fill: ansblue,
      text(size: 6.5pt, fill: white, top-edge: "x-height", bottom-edge: "baseline")[ans]))
  }))

