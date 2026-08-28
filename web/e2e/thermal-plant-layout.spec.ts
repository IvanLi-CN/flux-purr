import { expect, test } from '@playwright/test'

test.describe('thermal plant result layout', () => {
  test('does not let the trace consume the representative-point table after a desktop re-entry', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 960, height: 900 })
    await page.goto('/devices/fp-lab-01/calibration/heater-curve?demo=true')

    const resultCard = page.getByRole('article', { name: '自动热模型标定结果' })
    await expect(resultCard).toBeVisible({ timeout: 10_000 })
    await page.setViewportSize({ width: 1440, height: 900 })

    const samples = [] as Array<{
      documentHeight: number
      tableHeight: number
      traceHeight: number
    }>
    for (let index = 0; index < 12; index += 1) {
      samples.push(
        await resultCard.evaluate((element) => ({
          documentHeight: document.documentElement.scrollHeight,
          tableHeight:
            element.querySelector('.thermal-plant-run-card__table-wrap')?.getBoundingClientRect()
              .height ?? 0,
          traceHeight:
            element.querySelector('.thermal-plant-run-card__trace-panel')?.getBoundingClientRect()
              .height ?? 0,
        }))
      )
      await page.waitForTimeout(250)
    }

    const documentHeights = samples.map((sample) => sample.documentHeight)
    const tableHeights = samples.map((sample) => sample.tableHeight)
    const traceHeights = samples.map((sample) => sample.traceHeight)

    expect(Math.max(...traceHeights) - Math.min(...traceHeights)).toBeLessThanOrEqual(1)
    expect(Math.min(...tableHeights)).toBeGreaterThan(120)
    expect(Math.max(...documentHeights)).toBeLessThanOrEqual(900)
  })
})
