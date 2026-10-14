// keeps a canvas's backing store at the size it's shown times the screen's pixel ratio, at a fixed aspect, so
// a game drawn in its own units stays sharp however big its panel gets (full screen included). never smaller
// than the game's own units. returns the cleanup
export function fitCanvas(canvas: HTMLCanvasElement, w: number, h: number, onResize?: () => void): () => void {
  const fit = () => {
    const dpr = Math.min(3, Math.max(1, window.devicePixelRatio || 1))
    const shown = canvas.getBoundingClientRect().width
    const width = Math.max(w, Math.round((shown || w) * dpr))
    const height = Math.round((width * h) / w)
    if (canvas.width === width && canvas.height === height) return
    canvas.width = width
    canvas.height = height
    onResize?.()
  }
  fit()
  const ro = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(fit) : null
  ro?.observe(canvas)
  return () => ro?.disconnect()
}
