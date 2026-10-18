import { expect, test } from '@playwright/test'

const identity = 'fp-lab-01'

test.describe('public Demo routing', () => {
  test('restores deep links, tabs, refresh, and history without a document reload', async ({
    page,
  }) => {
    let documentLoads = 0
    page.on('request', (request) => {
      if (request.isNavigationRequest() && request.resourceType() === 'document') documentLoads += 1
    })

    await page.goto(`/devices/${identity}/overview`)
    await expect(page.getByRole('heading', { name: '热控工作台' })).toBeVisible()
    await page.getByRole('link', { name: /设置/ }).click()
    await expect(page).toHaveURL(`/devices/${identity}/settings/presets`)
    await page.getByRole('link', { name: /校准/ }).click()
    await expect(page).toHaveURL(`/devices/${identity}/calibration/heater-curve`)
    await page.reload()
    await expect(page.getByRole('tab', { name: '加热曲线标定' })).toHaveAttribute(
      'aria-current',
      'page'
    )
    await page.goBack()
    await expect(page).toHaveURL(`/devices/${identity}/settings/presets`)
    expect(documentLoads).toBe(2)
  })

  test('keeps unknown identities in the URL and offers recovery actions', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 812 })
    await page.goto('/devices/missing-device/settings')
    await expect(page).toHaveURL('/devices/missing-device/settings/presets')
    await expect(page.getByRole('heading', { name: 'Choose target' })).toBeVisible()
    await expect(page.getByRole('status').getByText('连接恢复')).toBeVisible()
    await expect(page.getByRole('button', { name: '重试恢复' })).toBeVisible()
  })

  test('blocks a target switch until calibration exits', async ({ page }) => {
    await page.goto(`/devices/${identity}/calibration/rtd-adc`)
    await page.getByRole('switch', { name: '标定模式' }).click()
    await page.getByRole('button', { name: '目标设备' }).click()
    await page.locator('[data-device-id="fp-kit-02"]').getByRole('button').first().click()
    await expect(page).toHaveURL(`/devices/${identity}/calibration/rtd-adc`)
    await expect(page.getByRole('dialog', { name: '校准未关闭' })).toBeVisible()
    await page.getByRole('button', { name: '关闭并继续' }).click()
    await expect(page).toHaveURL('/devices/fp-kit-02/calibration/rtd-adc')
  })

  test('cleans legacy mode inputs while staying on the public Demo', async ({ page }) => {
    await page.goto('/devices/missing-device/settings?uiDemo=firmware-workspace&demo=false')
    await expect(page).toHaveURL('/devices/missing-device/settings/presets')
    await expect(page.getByRole('heading', { name: 'Choose target' })).toBeVisible()
  })
})
