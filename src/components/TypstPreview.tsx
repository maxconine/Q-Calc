import { useEffect, useState } from 'react'
import { toTypstMath, typstDocument } from '../lib/typstMath'
import { isStaleRender, renderTypstSvg, warmupTypst } from '../lib/typstRender'

export function TypstPreview({ expr, answer, theme }: { expr: string; answer: string; theme: string }) {
  const [svg, setSvg] = useState('')

  useEffect(() => {
    warmupTypst()
  }, [])

  useEffect(() => {
    const root = document.documentElement
    const fill = getComputedStyle(root).getPropertyValue('--ink').trim() || '#1d1d1f'
    const doc = typstDocument(expr, fill, answer)
    if (!doc) return
    let cancelled = false
    renderTypstSvg(doc)
      .then((next) => {
        if (!cancelled) setSvg(next)
      })
      .catch((err: unknown) => {
        if (!cancelled && !isStaleRender(err)) setSvg('')
      })
    return () => {
      cancelled = true
    }
  }, [expr, answer, theme])

  if (!svg || !toTypstMath(expr)) return null
  return <div className="typst-preview" aria-hidden="true" dangerouslySetInnerHTML={{ __html: svg }} />
}
