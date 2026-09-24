import { identifyJob, type ClosedFormJob } from './closedForm'

self.onmessage = (event: MessageEvent<{ id: number; job: ClosedFormJob }>) => {
  const { id, job } = event.data
  let exact: string | null = null
  try {
    exact = identifyJob(job)
  } catch {
    exact = null
  }
  self.postMessage({ id, exact })
}
