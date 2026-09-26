import { expect, test } from '@playwright/test'

test('persists device event pages across a browser refresh', async ({ page }) => {
  await page.goto('/devices/fp-lab-01/calibration/firmware-tuning?demo=true')
  await expect(page.getByRole('heading', { name: '固件热控调优' })).toBeVisible()
  const deviceId = `journal-${crypto.randomUUID()}`
  await page.evaluate(async (device) => {
    const modulePath = '/src/features/control-plane-demo/thermal-tuning-browser-journal.ts'
    const journal = await import(/* @vite-ignore */ modulePath)
    const runId = 'journal-reload-check'
    const status = {
      runId,
      candidateId: 'candidate-5a',
      candidateHash: 'sha256:test',
      terminalDisposition: 'running',
      reviewDisposition: 'pending',
      trace: { firstSequence: 0, lastSequence: 0, complete: false, gap: false, digest: 'sha256:page-0' },
      events: { events: [], nextSequence: 0 },
    }
    const first = {
      runId,
      status,
      events: [{ sequence: 0, elapsedMs: 0, kind: 'phase_transition', phase: 'warmup' }],
      acknowledgedThrough: 0,
      confirmedThrough: null,
      gap: false,
      gapReason: null,
      complete: false,
    }
    await journal.persistThermalTuningRecording(device, first)
    const second = {
      ...first,
      status: {
        ...status,
        terminalDisposition: 'candidate_ready',
        trace: { ...status.trace, lastSequence: 1, complete: true, digest: 'sha256:page-1' },
      },
      events: [
        ...first.events,
        { sequence: 1, elapsedMs: 100, kind: 'decision', phase: 'hold' },
      ],
      acknowledgedThrough: 1,
      confirmedThrough: 1,
      complete: true,
    }
    await journal.persistThermalTuningRecording(device, second)
  }, deviceId)

  await page.reload()
  const restored = await page.evaluate(async (device) => {
    const modulePath = '/src/features/control-plane-demo/thermal-tuning-browser-journal.ts'
    const journal = await import(/* @vite-ignore */ modulePath)
    const recording = await journal.loadThermalTuningRecording(device)
    return {
      sequences: recording?.events.map((event: { sequence: number }) => event.sequence),
      complete: recording?.complete,
      candidateHash: recording?.status.candidateHash,
    }
  }, deviceId)
  expect(restored).toEqual({
    sequences: [0, 1],
    complete: true,
    candidateHash: 'sha256:test',
  })
})
