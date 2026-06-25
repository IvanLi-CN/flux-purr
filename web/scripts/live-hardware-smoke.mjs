import { chromium } from 'playwright'

function parseArgs(argv) {
  const options = {
    pageUrl: 'http://127.0.0.1:20501/?demo=false',
    apiBaseUrl: 'http://127.0.0.1:20500',
    durationMs: 180_000,
    tickMs: 5_000,
    scenario: 'rtd-dashboard',
  }

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index]
    const next = argv[index + 1]
    if (arg === '--page-url' && next) {
      options.pageUrl = next
      index += 1
      continue
    }
    if (arg === '--api-base-url' && next) {
      options.apiBaseUrl = next
      index += 1
      continue
    }
    if (arg === '--duration-ms' && next) {
      options.durationMs = Number(next)
      index += 1
      continue
    }
    if (arg === '--tick-ms' && next) {
      options.tickMs = Number(next)
      index += 1
      continue
    }
    if (arg === '--scenario' && next) {
      options.scenario = next
      index += 1
      continue
    }
    throw new Error(`Unknown or incomplete argument: ${arg}`)
  }

  return options
}

function nowSince(startedAtMs) {
  return Date.now() - startedAtMs
}

function asMiB(bytes) {
  return Number((bytes / (1024 * 1024)).toFixed(2))
}

function matchesBadMarker(text) {
  const markers = ['No live target', 'Failed to fetch', 'Heater curve unavailable', 'Choose target']
  return markers.filter((marker) => text.includes(marker))
}

async function sleep(ms) {
  await new Promise((resolve) => setTimeout(resolve, ms))
}

async function waitForReady(page, startedAtMs) {
  for (let attempt = 0; attempt < 15; attempt += 1) {
    const text = await page.locator('body').innerText()
    if (text.includes('运行时已同步') && text.includes('/ DEVD')) {
      return {
        ok: true,
        recoveredAtMs: nowSince(startedAtMs),
      }
    }
    await sleep(1_000)
  }

  return {
    ok: false,
    recoveredAtMs: null,
  }
}

async function snapshotDevice(page, apiBaseUrl) {
  const response = await page.request.get(`${apiBaseUrl}/api/v1/devices`)
  const payload = await response.json()
  if (!Array.isArray(payload?.devices) || payload.devices.length === 0) {
    return null
  }
  return payload.devices[0]
}

async function snapshotPageMetrics(cdp) {
  if (!cdp) {
    return null
  }

  try {
    await cdp.send('HeapProfiler.enable').catch(() => undefined)
    await cdp.send('HeapProfiler.collectGarbage').catch(() => undefined)
    const { metrics } = await cdp.send('Performance.getMetrics')
    const metricMap = new Map(metrics.map((entry) => [entry.name, entry.value]))
    return {
      documents: metricMap.get('Documents') ?? null,
      nodes: metricMap.get('Nodes') ?? null,
      jsHeapUsedSizeMiB: metricMap.has('JSHeapUsedSize')
        ? asMiB(metricMap.get('JSHeapUsedSize'))
        : null,
      jsHeapTotalSizeMiB: metricMap.has('JSHeapTotalSize')
        ? asMiB(metricMap.get('JSHeapTotalSize'))
        : null,
      eventListeners: metricMap.get('JSEventListeners') ?? null,
      frames: metricMap.get('Frames') ?? null,
    }
  } catch {
    return null
  }
}

function summarizeDeviceStatus(device) {
  if (!device?.status) {
    return null
  }

  const status = device.status
  return {
    mode: status.mode ?? null,
    targetTempC: status.targetTempC ?? null,
    currentTempC: status.currentTempC ?? null,
    heaterEnabled: status.heaterEnabled ?? null,
    heaterOutputPercent: status.heaterOutputPercent ?? null,
    pdState: status.pdState ?? null,
    pdContractMv: status.pdContractMv ?? null,
    voltageMv: status.voltageMv ?? null,
    uptimeSeconds: status.uptimeSeconds ?? null,
    calibration: status.calibration
      ? {
          mode: status.calibration.mode ?? null,
          ppsEnabled: status.calibration.ppsEnabled ?? null,
          heaterEnabled: status.calibration.heaterEnabled ?? null,
          targetAdcMv: status.calibration.targetAdcMv ?? null,
          stable: status.calibration.stable ?? null,
          stabilityErrorMv: status.calibration.stabilityErrorMv ?? null,
          job: status.calibration.job
            ? {
                kind: status.calibration.job.kind ?? null,
                status: status.calibration.job.status ?? null,
                progressPercent: status.calibration.job.progressPercent ?? null,
              }
            : null,
        }
      : null,
    network: status.network
      ? {
          state: status.network.state ?? null,
          lastError: status.network.lastError ?? null,
        }
      : null,
  }
}

function summarizePageMetrics(samples) {
  const usableSamples = samples.filter(Boolean)
  if (usableSamples.length === 0) {
    return null
  }

  const used = usableSamples
    .map((sample) => sample.jsHeapUsedSizeMiB)
    .filter((value) => typeof value === 'number')
  const total = usableSamples
    .map((sample) => sample.jsHeapTotalSizeMiB)
    .filter((value) => typeof value === 'number')
  const nodes = usableSamples
    .map((sample) => sample.nodes)
    .filter((value) => typeof value === 'number')

  return {
    sampleCount: usableSamples.length,
    jsHeapUsedSizeMiB:
      used.length > 0
        ? {
            min: Math.min(...used),
            max: Math.max(...used),
            delta: Number((used[used.length - 1] - used[0]).toFixed(2)),
          }
        : null,
    jsHeapTotalSizeMiB:
      total.length > 0
        ? {
            min: Math.min(...total),
            max: Math.max(...total),
            delta: Number((total[total.length - 1] - total[0]).toFixed(2)),
          }
        : null,
    nodes:
      nodes.length > 0
        ? {
            min: Math.min(...nodes),
            max: Math.max(...nodes),
            delta: nodes[nodes.length - 1] - nodes[0],
          }
        : null,
  }
}

async function enterRtdCalibration(page) {
  await page.getByRole('button', { name: '校准' }).click()
  await page.getByRole('tab', { name: '温度标定' }).click()
  await page
    .getByRole('heading', { name: '温度 ADC' })
    .waitFor({ state: 'visible', timeout: 10_000 })
}

async function runRtdDashboardScenario(page) {
  await enterRtdCalibration(page)
  const targetAdcInput = page.getByLabel('目标 ADC 输入')
  const modeSwitch = page.getByRole('switch', { name: '标定模式' })

  await targetAdcInput.fill('950')
  if ((await modeSwitch.getAttribute('data-state')) !== 'checked') {
    await modeSwitch.click()
    await sleep(700)
  }

  const ppsButton = page.getByRole('button', { name: /申请 PPS|关闭 PPS/ })
  const ppsLabel = await ppsButton.textContent()
  if (ppsLabel?.includes('申请 PPS')) {
    await ppsButton.click()
    await sleep(700)
  }

  const heaterButton = page.getByRole('button', { name: /开启加热|关闭加热/ })
  const heaterLabel = await heaterButton.textContent()
  if (heaterLabel?.includes('开启加热')) {
    await heaterButton.click()
    await sleep(700)
  }

  await targetAdcInput.fill('980')
  await sleep(1_200)

  await page.getByRole('button', { name: '总览' }).click()
  const dashboardTarget = page.getByLabel('Dashboard target temperature')
  await dashboardTarget.fill('50')
  await sleep(1_000)
  await dashboardTarget.fill('55')
  await sleep(1_000)

  await enterRtdCalibration(page)

  return {
    targetAdcInput,
    dashboardTarget,
  }
}

async function main() {
  const options = parseArgs(process.argv.slice(2))
  const startedAtMs = Date.now()
  const browser = await chromium.launch({ headless: true })
  const page = await browser.newPage({ viewport: { width: 1600, height: 1100 } })
  const cdp = await page
    .context()
    .newCDPSession(page)
    .catch(() => null)
  if (cdp) {
    await cdp.send('Performance.enable').catch(() => undefined)
  }

  const consoleProblems = []
  const pageErrors = []
  const requestFailures = []
  const crashEvents = []
  const snapshots = []
  const pageMetricSamples = []

  page.on('console', (message) => {
    const text = message.text()
    if (message.type() === 'error' || message.type() === 'warning' || /fail|error/i.test(text)) {
      consoleProblems.push({
        t: nowSince(startedAtMs),
        type: message.type(),
        text,
      })
    }
  })
  page.on('pageerror', (error) => {
    pageErrors.push({
      t: nowSince(startedAtMs),
      message: String(error),
    })
  })
  page.on('requestfailed', (request) => {
    requestFailures.push({
      t: nowSince(startedAtMs),
      method: request.method(),
      url: request.url(),
      error: request.failure()?.errorText ?? 'unknown',
    })
  })
  page.on('crash', () => {
    crashEvents.push({
      t: nowSince(startedAtMs),
      message: 'page crashed',
    })
  })

  let targetAdcInput = null
  let dashboardTarget = null

  try {
    await page.goto(options.pageUrl, { waitUntil: 'domcontentloaded', timeout: 30_000 })
    await page.locator('body').waitFor({ state: 'visible', timeout: 15_000 })

    const ready = await waitForReady(page, startedAtMs)
    if (!ready.ok) {
      throw new Error('Live page did not recover from the initial devd reconnect window.')
    }

    if (options.scenario === 'rtd-dashboard') {
      ;({ targetAdcInput, dashboardTarget } = await runRtdDashboardScenario(page))
    } else if (options.scenario === 'rtd-soak') {
      await enterRtdCalibration(page)
      targetAdcInput = page.getByLabel('目标 ADC 输入')
    } else {
      throw new Error(`Unsupported scenario: ${options.scenario}`)
    }

    while (nowSince(startedAtMs) < options.durationMs) {
      await sleep(options.tickMs)
      const text = await page.locator('body').innerText()
      const badMarkers = matchesBadMarker(text)
      const device = await snapshotDevice(page, options.apiBaseUrl)
      const pageMetrics = await snapshotPageMetrics(cdp)
      pageMetricSamples.push(pageMetrics)
      snapshots.push({
        t: nowSince(startedAtMs),
        badMarkers,
        reconnecting: text.includes('正在重新接管本机 devd 租约，请稍候。'),
        excerpt:
          badMarkers.length > 0 || text.includes('正在重新接管本机 devd 租约，请稍候。')
            ? text.slice(0, 1_200)
            : undefined,
        deviceStatus: summarizeDeviceStatus(device),
        pageMetrics,
      })
    }

    const finalBody = await page.locator('body').innerText()
    const finalBadMarkers = matchesBadMarker(finalBody)
    const finalDevice = await snapshotDevice(page, options.apiBaseUrl)
    const finalPageMetrics = await snapshotPageMetrics(cdp)
    pageMetricSamples.push(finalPageMetrics)
    const result = {
      ok:
        crashEvents.length === 0 &&
        pageErrors.length === 0 &&
        requestFailures.length === 0 &&
        consoleProblems.length === 0 &&
        snapshots.every((snapshot) => snapshot.badMarkers.length === 0 && !snapshot.reconnecting) &&
        finalBadMarkers.length === 0,
      pageUrl: options.pageUrl,
      apiBaseUrl: options.apiBaseUrl,
      scenario: options.scenario,
      durationMs: nowSince(startedAtMs),
      targetAdcInputValue: targetAdcInput
        ? await targetAdcInput.inputValue().catch(() => null)
        : null,
      dashboardTargetValue: dashboardTarget
        ? await dashboardTarget.inputValue().catch(() => null)
        : null,
      crashEvents,
      pageErrors,
      requestFailures,
      consoleProblems,
      finalBadMarkers,
      finalDeviceStatus: summarizeDeviceStatus(finalDevice),
      finalPageMetrics,
      pageMetricsSummary: summarizePageMetrics(pageMetricSamples),
      snapshots,
    }

    console.log(JSON.stringify(result, null, 2))
  } finally {
    await browser.close()
  }
}

await main()
