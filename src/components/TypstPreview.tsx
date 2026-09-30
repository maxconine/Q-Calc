import { memo, useEffect, useMemo, useState } from 'react'
import { typstDocument } from '../lib/typstMath'
import { isStaleRender, renderTypstSvg, warmupTypst } from '../lib/typstRender'

type Shown = { svg: string; doc: string }

// The svg is black glyphs made currentColor, so the css text color themes it, light or dark,
// without compiling again. Mid-edit input that Typst can't compile keeps the last render, dimmed,
// so the preview doesn't blink out (and the window doesn't jump) on every keystroke.
export const TypstPreview = memo(function TypstPreview({
  expr,
  answer,
  solvedFor = '',
}: {
  expr: string
  answer: string
  solvedFor?: string
}) {
  const [shown, setShown] = useState<Shown | null>(null)
  const [failed, setFailed] = useState('')
  const doc = useMemo(() => typstDocument(expr, answer, solvedFor), [expr, answer, solvedFor])

  useEffect(() => {
    warmupTypst()
  }, [])

  useEffect(() => {
    if (!doc) {
      setShown(null)
      return
    }
    let cancelled = false
    renderTypstSvg(doc)
      .then((svg) => {
        if (!cancelled) setShown({ svg, doc })
      })
      .catch((err: unknown) => {
        if (!cancelled && !isStaleRender(err)) setFailed(doc)
      })
    return () => {
      cancelled = true
    }
  }, [doc])

  if (!doc || !shown) return null
  const stale = shown.doc !== doc && failed === doc
  return (
    <div
      className={`typst-preview${stale ? ' typst-preview-stale' : ''}`}
      aria-hidden="true"
      dangerouslySetInnerHTML={{ __html: shown.svg }}
    />
  )
})
