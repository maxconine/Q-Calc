import { CAM_EASE, CAM_MARGIN_X, CAM_MARGIN_Y, CAM_MIN_ZOOM, PLAYER_H, PLAYER_W, VIEW_H, VIEW_W, type Camera, type CameraFn } from './emberTypes'

// ember & frost's camera: which part of a stage is on screen. pure, so the renderer and the tests agree.
// it frames both players with some room around them, never closer than one view, and zooms out as they
// split up. a one-screen stage on a 16:9 canvas is simply the whole stage at zoom 1

// the smallest rect of this aspect that holds one view: what zoom 1 shows
export function baseView(aspect: number): { w: number; h: number } {
  const a = Number.isFinite(aspect) && aspect > 0 ? aspect : VIEW_W / VIEW_H
  const h = Math.max(VIEW_H, VIEW_W / a)
  return { w: h * a, h }
}

// one axis: a span of `size` centred on `mid`, kept inside 0..limit, or centred over it when it's the bigger
const place = (mid: number, size: number, limit: number) => (size >= limit ? (limit - size) / 2 : Math.min(limit - size, Math.max(0, mid - size / 2)))

// where the camera wants to be for this state. dead players count where they fell, so the view doesn't lurch
export const cameraFor: CameraFn = (stage, s, aspect) => {
  const base = baseView(aspect)
  const [a, b] = s.players
  const x0 = Math.min(a.x, b.x)
  const x1 = Math.max(a.x, b.x) + PLAYER_W
  const y0 = Math.min(a.y, b.y)
  const y1 = Math.max(a.y, b.y) + PLAYER_H
  // how many views wide the frame has to be to hold both with their margins
  const need = Math.max(1, (x1 - x0 + 2 * CAM_MARGIN_X) / base.w, (y1 - y0 + 2 * CAM_MARGIN_Y) / base.h)
  // never wider than the whole stage needs, nor than the widest zoom (which holds the biggest stage there is)
  const whole = Math.max(1, stage.w / base.w, stage.h / base.h)
  const k = Math.min(need, whole, 1 / CAM_MIN_ZOOM)
  const w = base.w * k
  const h = base.h * k
  return { x: place((x0 + x1) / 2, w, stage.w), y: place((y0 + y1) / 2, h, stage.h), w, h, zoom: 1 / k }
}

// the drawn camera closing on its target over `ticks` steps. from null (a new stage, a respawn far away) it jumps
export function easeCamera(from: Camera | null, to: Camera, ticks = 1): Camera {
  if (!from || from.w / from.h !== to.w / to.h) return to
  const t = 1 - Math.pow(1 - CAM_EASE, Math.max(0, ticks))
  const mix = (p: number, q: number) => (Math.abs(q - p) < 0.01 ? q : p + (q - p) * t)
  const w = mix(from.w, to.w)
  return { x: mix(from.x, to.x), y: mix(from.y, to.y), w, h: w * (to.h / to.w), zoom: to.zoom * (to.w / w) }
}

// world px to canvas px for a canvas `width` px wide showing `cam`
export function cameraScale(cam: Camera, width: number): number {
  return width / cam.w
}
