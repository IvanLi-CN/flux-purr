import { expect, test } from '@playwright/test'

test.describe('calibration layout', () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 1000 })
    await page.goto('/')

    await page.locator('.industrial-view-tab').filter({ hasText: '校准' }).click()
    await page.getByText('温度标定', { exact: true }).click()
  })

  test('keeps the temperature status summary rows away from list edges', async ({ page }) => {
    const metrics = await page.evaluate(() => {
      const statusCard = [...document.querySelectorAll('.industrial-calibration-live-card')].find(
        (element) =>
          element
            .querySelector('.industrial-calibration-live-card__title-main')
            ?.textContent?.trim() === '状态'
      )
      const fitList = statusCard?.querySelector('.industrial-calibration-property-list')
      const rows = [
        ...(fitList?.querySelectorAll('.industrial-calibration-property-list__fit-group') ?? []),
      ]

      if (
        !(statusCard instanceof HTMLElement) ||
        !(fitList instanceof HTMLElement) ||
        rows.length === 0
      ) {
        return null
      }

      const listRect = fitList.getBoundingClientRect()

      return rows.map((row) => {
        const label = row.querySelector('dt')
        const value = row.querySelector('dd')

        if (!(label instanceof HTMLElement) || !(value instanceof HTMLElement)) {
          return null
        }

        const labelRect = label.getBoundingClientRect()
        const valueRect = value.getBoundingClientRect()

        return {
          labelInset: labelRect.left - listRect.left,
          valueInset: listRect.right - valueRect.right,
        }
      })
    })

    expect(metrics).not.toBeNull()
    for (const row of metrics ?? []) {
      expect(row).not.toBeNull()
      expect(row?.labelInset).toBeGreaterThanOrEqual(12)
      expect(row?.valueInset).toBeGreaterThanOrEqual(12)
    }
  })

  test('places ADC calibration commands under the status card', async ({ page }) => {
    const metrics = await page.evaluate(() => {
      const statusCard = [...document.querySelectorAll('.industrial-calibration-live-card')].find(
        (element) =>
          element
            .querySelector('.industrial-calibration-live-card__title-main')
            ?.textContent?.trim() === '状态'
      )
      const toolbar = document.querySelector('.industrial-calibration-adc-toolbar')
      const samples = document.querySelector('.industrial-calibration-channel--samples')

      if (
        !(statusCard instanceof HTMLElement) ||
        !(toolbar instanceof HTMLElement) ||
        !(samples instanceof HTMLElement)
      ) {
        return null
      }

      const statusRect = statusCard.getBoundingClientRect()
      const toolbarRect = toolbar.getBoundingClientRect()
      const samplesRect = samples.getBoundingClientRect()

      return {
        toolbarBelowStatus: toolbarRect.top >= statusRect.bottom,
        toolbarAboveSamples: toolbarRect.bottom <= samplesRect.top,
        toolbarAlignedWithStatus: Math.abs(toolbarRect.left - statusRect.left),
        toolbarWidthDelta: Math.abs(toolbarRect.width - statusRect.width),
        labels: [...toolbar.querySelectorAll('button')].map((button) => button.textContent?.trim()),
      }
    })

    expect(metrics).not.toBeNull()
    expect(metrics?.toolbarBelowStatus).toBe(true)
    expect(metrics?.toolbarAboveSamples).toBe(true)
    expect(metrics?.toolbarAlignedWithStatus).toBeLessThanOrEqual(1)
    expect(metrics?.toolbarWidthDelta).toBeLessThanOrEqual(1)
    expect(metrics?.labels).toEqual(['导出', '导入'])
  })
})
