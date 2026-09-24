import { useEffect, useRef, useState } from 'react'
import { jobKey, knownClosedForm, rememberClosedForm } from '../engine/calculus'
import type { ClosedFormJob } from '../engine/closedForm'
import ClosedFormWorker from '../engine/closedForm.worker?worker&inline'

// waits for typing to pause, so the worker isn't restarted on every keystroke
const SETTLE_MS = 150

type Request = { id: number; key: string; job: ClosedFormJob; done: (exact: string | null) => void }

let worker: Worker | null = null
let running: Request | null = null
let lastId = 0

function stopWorker(): void {
  worker?.terminate()
  worker = null
  running = null
}

function startWorker(): Worker | null {
  if (worker) return worker
  if (typeof Worker === 'undefined') return null
  try {
    worker = new ClosedFormWorker()
  } catch {
    return null
  }
  worker.onmessage = (event: MessageEvent<{ id: number; exact: string | null }>) => {
    const req = running
    if (!req || event.data.id !== req.id) return
    running = null
    rememberClosedForm(req.job, event.data.exact)
    req.done(event.data.exact)
  }
  worker.onerror = () => stopWorker()
  return worker
}

function request(job: ClosedFormJob, key: string, done: (exact: string | null) => void): void {
  if (running?.key === key) {
    running.done = done
    return
  }
  // a search for an answer nobody is looking at any more is thrown away
  if (running) stopWorker()
  const w = startWorker()
  if (!w) return
  running = { id: ++lastId, key, job, done }
  w.postMessage({ id: running.id, job })
}

/** The worker's verified closed form for the live answer, once it has one. */
export function useClosedForm(job: ClosedFormJob | undefined): string | undefined {
  const key = job ? jobKey(job) : ''
  const jobRef = useRef(job)
  jobRef.current = job
  const [found, setFound] = useState<{ key: string; exact: string } | null>(null)
  useEffect(() => {
    const current = jobRef.current
    if (!key || !current || knownClosedForm(key) !== undefined) return
    const t = window.setTimeout(() => {
      request(current, key, (exact) => {
        if (exact) setFound({ key, exact })
      })
    }, SETTLE_MS)
    return () => window.clearTimeout(t)
  }, [key])
  if (!key) return undefined
  return found?.key === key ? found.exact : (knownClosedForm(key) ?? undefined)
}
