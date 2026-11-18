import { expect, test } from '@playwright/test'

// Use Chromium's full browser in headless mode, matching the installed CI browser.
test.use({ channel: 'chromium' })

test('stops active tuning reads at the retry budget and retains the last snapshot', async ({
  page,
}) => {
  const errors: string[] = []
  page.on('pageerror', (error) => errors.push(error.message))
  await page.goto('/e2e/fixtures/firmware-tuning-polling.html')
  const reads = page.getByTestId('read-count')
  // One successful active-run read, followed by three failures.
  await expect(reads).toHaveText('4')
  await expect(page.getByText('Fixture tuning read unavailable')).toBeVisible()
  await expect(page.locator('.firmware-tuning__facts').getByText('warmup')).toBeVisible()
  await expect(page.getByRole('button', { name: '停止', exact: true })).toBeEnabled()
  await expect(page.getByText('高级选项：导入候选', { exact: true })).toHaveCount(0)
  await page.waitForTimeout(2_500)
  await expect(reads).toHaveText('4')

  await page.getByRole('button', { name: '重试读取' }).click()
  await expect(reads).toHaveText('7')
  await page.waitForTimeout(2_500)
  await expect(reads).toHaveText('7')
  await expect(page.getByTestId('write-count')).toHaveText('0')
  // Read recovery must not revoke the safety stop for the retained active run.
  const stop = page.getByRole('button', { name: '停止', exact: true })
  await expect(stop).toBeEnabled()
  await stop.click()
  await expect(page.getByTestId('write-count')).toHaveText('1')
  await expect(page.getByTestId('write-count')).toHaveAttribute(
    'data-command',
    JSON.stringify({ op: 'cancel', runId: 'polling-fixture-run' })
  )
  expect(errors).toEqual([])
})

test('saves a result in one click, including acknowledgement of the seal event', async ({
  page,
}) => {
  await page.goto('/e2e/fixtures/firmware-tuning-polling.html?review=owner')
  await page.getByRole('button', { name: '保存', exact: true }).click()
  await expect(page.locator('.firmware-tuning__facts').getByText('已保存')).toBeVisible()
  await expect(page.getByTestId('write-count')).toHaveText('2')
  await expect(page.getByRole('button', { name: '封存候选' })).toHaveCount(0)
})

test('discards without saving and restores the option to start', async ({ page }) => {
  await page.goto('/e2e/fixtures/firmware-tuning-polling.html?review=observer')
  await expect(page.getByText('请在发起本轮调优的页面保存。')).toBeVisible()
  await expect(page.getByRole('button', { name: '保存', exact: true })).toBeDisabled()
  const importOption = page.getByText('高级选项：导入候选', { exact: true })
  await expect(importOption).toHaveCount(0)
  await page.getByRole('button', { name: '放弃', exact: true }).click()
  const confirmation = page.getByRole('dialog', { name: '放弃本轮调优结果？' })
  await expect(confirmation).toBeVisible()
  await expect(page.getByTestId('write-count')).toHaveText('0')
  await confirmation.getByRole('button', { name: '取消', exact: true }).click()
  await expect(confirmation).toBeHidden()
  await expect(page.getByTestId('write-count')).toHaveText('0')
  await page.getByRole('button', { name: '放弃', exact: true }).click()
  await confirmation.getByRole('button', { name: '确认放弃' }).click()
  await expect(page.getByRole('button', { name: '开始调优' })).toBeEnabled()
  await expect(importOption).toBeVisible()
  await expect(page.getByTestId('write-count')).toHaveText('1')
})

test('plots recorded temperatures by target and trial and keeps device events available', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1280, height: 860 })
  await page.goto('/e2e/fixtures/firmware-tuning-polling.html?chart')
  const targets = page.getByRole('radiogroup', { name: '曲线目标温度' })
  const target80 = targets.getByRole('radio', { name: '80°C' })
  const target60 = targets.getByRole('radio', { name: '60°C' })
  const trial = page.getByRole('combobox', { name: '曲线试验轮次' })
  await expect(target80).toBeChecked()
  await expect(page.getByText('峰值 80.5 °C · 超调 0.5 °C')).toBeVisible()
  const phaseLegend = page.locator('.firmware-tuning__chart-legend')
  await expect(phaseLegend).toHaveText('预热接近目标稳温')
  await expect(page.getByRole('img', { name: /80 °C 温度曲线/ }).getByText('3.0 s')).toBeVisible()
  await expect(page.getByText('稳温峰峰值 未记录')).toBeVisible()
  await expect(trial.getByRole('img', { name: '最佳' })).toHaveCount(0)
  await target80.focus()
  await page.keyboard.down('ArrowLeft')
  await expect(target60).toBeChecked()
  await page.keyboard.up('ArrowLeft')
  await target80.click()
  await expect(target80).toBeChecked()
  await target60.click()
  await expect(page.getByText('峰值 60.0 °C · 超调 0.0 °C')).toBeVisible()
  await expect(page.getByText('稳温峰峰值 0.34 °C')).toBeVisible()
  await expect(trial.getByRole('img', { name: '最佳' })).toBeVisible()
  const centerOffset = (icon: Element) => {
    const text = icon.parentElement?.querySelector('.firmware-tuning__trial-text')
    if (!text) throw new Error('Missing trial label')
    const textRect = text.getBoundingClientRect()
    const iconRect = icon.getBoundingClientRect()
    return Math.abs(textRect.top + textRect.height / 2 - iconRect.top - iconRect.height / 2)
  }
  expect(await trial.getByRole('img', { name: '最佳' }).evaluate(centerOffset)).toBeLessThan(1)
  const trialWidth = await trial.evaluate((node) => node.getBoundingClientRect().width)
  await trial.click()
  const optionStar = page
    .getByRole('option', { name: /第 2 次/ })
    .getByRole('img', { name: '最佳' })
  await expect(optionStar).toBeVisible()
  expect(await optionStar.evaluate(centerOffset)).toBeLessThan(1)
  await page.getByRole('option', { name: '第 1 次', exact: true }).click()
  await expect(trial.getByRole('img', { name: '最佳' })).toHaveCount(0)
  expect(await trial.evaluate((node) => node.getBoundingClientRect().width)).toBe(trialWidth)
  await expect(page.getByText('峰值 62.2 °C · 超调 2.2 °C')).toBeVisible()
  await expect(page.getByText('稳温峰峰值 未记录')).toBeVisible()
  await expect(page.getByRole('img', { name: /60 °C 温度曲线/ }).locator('polyline')).toBeVisible()
  const dimensions = await page
    .locator('.industrial-calibration-tabs__firmware-scroll')
    .evaluate((area) => {
      const plot = area.querySelector('.firmware-tuning__chart-plot')?.getBoundingClientRect()
      const scroll = area.querySelector('.simplebar-content-wrapper')
      return {
        bottomInset: area.getBoundingClientRect().bottom - (plot?.bottom ?? 0),
        plotHeight: plot?.height ?? 0,
        overflow: scroll ? scroll.scrollHeight - scroll.clientHeight : Infinity,
      }
    })
  expect(dimensions.plotHeight).toBeGreaterThan(200)
  expect(dimensions.bottomInset).toBeLessThan(24)
  expect(dimensions.overflow).toBeLessThanOrEqual(1)
  await page.getByRole('button', { name: '设备事件', exact: true }).click()
  await expect(page.getByRole('columnheader', { name: '时间' })).toBeVisible()
})

test('keeps only the ten latest runs across refresh without persisting owner proofs', async ({
  page,
}) => {
  const fixture = '/e2e/fixtures/firmware-tuning-polling.html'
  await page.route(`**${fixture}`, (route) =>
    route.fulfill({ contentType: 'text/html', body: '<title>Journal fixture</title>' })
  )
  await page.goto(fixture)
  await page.evaluate(async () => {
    const path = '/src/features/control-plane-demo/thermal-tuning-browser-journal.ts'
    const journal = await import(/* @vite-ignore */ path)
    for (let number = 0; number <= 10; number += 1) {
      const runId = `run-${number}`
      const event = { sequence: 0, kind: 'decision' }
      await journal.persistThermalTuningRecording('fixture', {
        runId,
        status: { runId, ownerProof: 'fixture-secret' },
        events: [event],
      })
    }
  })
  await page.reload()
  const archive = await page.evaluate(async () => {
    const path = '/src/features/control-plane-demo/thermal-tuning-browser-journal.ts'
    const journal = await import(/* @vite-ignore */ path)
    const runs = await Promise.all(
      Array.from({ length: 11 }, (_, number) =>
        journal.loadThermalTuningRecording('fixture', `run-${number}`)
      )
    )
    return {
      runs: runs.map((run) => run?.runId ?? null),
      intact: runs
        .slice(1)
        .every((run) => run?.events[0]?.kind === 'decision' && !('ownerProof' in run.status)),
    }
  })
  expect(archive).toEqual({
    runs: [null, ...Array.from({ length: 10 }, (_, index) => `run-${index + 1}`)],
    intact: true,
  })
})
