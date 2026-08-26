import { expect, test } from '@playwright/test'

const identity = 'fp-lab-01'

test.describe('device-scoped routing', () => {
  test('deep links, tabs, refresh, and browser history keep route state', async ({ page }) => {
    await page.goto(`/devices/${identity}/overview?demo=true`)
    await expect(page.getByRole('heading', { name: '热控工作台' })).toBeVisible()
    await expect(page).toHaveURL(new RegExp(`/devices/${identity}/overview\\?demo=true$`))

    await page.getByRole('link', { name: /设置/ }).click()
    await expect(page).toHaveURL(new RegExp(`/devices/${identity}/settings\\?demo=true$`))
    await expect(page.getByRole('link', { name: /设置/ })).toHaveAttribute('aria-current', 'page')

    await page.getByRole('link', { name: /校准/ }).click()
    await expect(page).toHaveURL(
      new RegExp(`/devices/${identity}/calibration/heater-curve\\?demo=true$`)
    )
    const heaterCurveTab = page.getByRole('tab', { name: '加热曲线标定' })
    await heaterCurveTab.focus()
    await heaterCurveTab.press('ArrowRight')
    await expect(page).toHaveURL(
      new RegExp(`/devices/${identity}/calibration/rtd-adc\\?demo=true$`)
    )

    await page.reload()
    await expect(page.getByRole('tab', { name: '温度标定' })).toHaveAttribute(
      'aria-current',
      'page'
    )
    await page.goBack()
    await expect(page).toHaveURL(
      new RegExp(`/devices/${identity}/calibration/heater-curve\\?demo=true$`)
    )
    await page.goForward()
    await expect(page).toHaveURL(
      new RegExp(`/devices/${identity}/calibration/rtd-adc\\?demo=true$`)
    )
  })

  test('keeps an unknown stable identity in the URL and offers recovery', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 812 })
    await page.goto('/devices/missing-device/settings?demo=true')
    await expect(page).toHaveURL(/\/devices\/missing-device\/settings\?demo=true$/)
    await expect(page.getByRole('heading', { name: 'Choose target' })).toBeVisible()
    await expect(page.getByRole('status').getByText('连接恢复')).toBeVisible()
    await expect(page.getByRole('button', { name: '重试恢复' })).toBeVisible()
    await expect(
      page.getByRole('region', { name: 'Add device' }).getByRole('button', { name: /Web Serial/ })
    ).toBeVisible()
    const actionHeights = await page
      .getByRole('status')
      .getByRole('button')
      .evaluateAll((buttons) => buttons.map((button) => button.getBoundingClientRect().height))
    expect(actionHeights.every((height) => height >= 48)).toBe(true)
  })

  test('keeps a known offline identity in the URL and offers recovery', async ({ page }) => {
    await page.goto('/devices/fp-demo-03/overview?demo=true')

    await expect(page).toHaveURL(/\/devices\/fp-demo-03\/overview\?demo=true$/)
    await expect(page.getByRole('heading', { name: 'Choose target' })).toBeVisible()
    await expect(page.getByRole('status').getByText('连接恢复')).toBeVisible()
    await expect(page.getByRole('button', { name: '重试恢复' })).toBeVisible()
  })

  test('pushes one history entry for a calibration tab mouse click', async ({ page }) => {
    await page.goto(`/devices/${identity}/calibration/heater-curve?demo=true`)

    await page.getByRole('tab', { name: '温度标定' }).click()
    await expect(page).toHaveURL(
      new RegExp(`/devices/${identity}/calibration/rtd-adc\\?demo=true$`)
    )
    await page.goBack()
    await expect(page).toHaveURL(
      new RegExp(`/devices/${identity}/calibration/heater-curve\\?demo=true$`)
    )
  })

  test('redirects indexes with replace and preserves typed search', async ({ page }) => {
    await page.goto('/devices?demo=true')
    await expect(page).toHaveURL(/\/devices\/new\?demo=true$/)

    await page.goto(`/devices/${identity}/overview?demo=true`)
    await expect(page.getByRole('heading', { name: '热控工作台' })).toBeVisible()

    await page.goto(`/devices/${identity}?demo=true`)
    await expect(page).toHaveURL(new RegExp(`/devices/${identity}/overview\\?demo=true$`))

    await page.goto(`/devices/${identity}/calibration?demo=true`)
    await expect(page).toHaveURL(
      new RegExp(`/devices/${identity}/calibration/heater-curve\\?demo=true$`)
    )

    await page.goto('/?demo=true')
    await expect(page).toHaveURL(new RegExp(`/devices/${identity}/overview\\?demo=true$`))
  })

  test('blocks a routed device switch until calibration exits', async ({ page }) => {
    await page.goto(`/devices/${identity}/calibration/heater-curve?demo=true`)
    await page.getByRole('switch', { name: '标定模式' }).click()

    await page.getByRole('button', { name: '目标设备' }).click()
    await page.locator('[data-device-id="fp-kit-02"]').getByRole('button').first().click()

    await expect(page).toHaveURL(
      new RegExp(`/devices/${identity}/calibration/heater-curve\\?demo=true$`)
    )
    await expect(page.getByRole('dialog', { name: '校准未关闭' })).toBeVisible()
    await page.getByRole('button', { name: '关闭并继续' }).click()
    await expect(page).toHaveURL(/\/devices\/fp-kit-02\/calibration\/heater-curve\?demo=true$/)
  })

  test('shows a router 404 for structurally invalid paths', async ({ page }) => {
    await page.goto('/devices/fp-lab-01/calibration/unknown?demo=true')
    await expect(page.getByRole('heading', { name: '路径不存在' })).toBeVisible()
  })

  test('normalizes bare UI demo search from any path and renders the mock surface', async ({
    page,
  }) => {
    await page.goto('/devices/missing-device/settings?uiDemo&demo=true')

    await expect(page).toHaveURL(/\/\?(?=.*demo=true)(?=.*uiDemo=true)/)
    await expect(page.getByLabel('LAN pairing demo')).toBeVisible()
    await expect(page.getByLabel('设备地址')).toHaveValue('http://192.168.1.18')
  })

  test('normalizes UI demo search before rendering an invalid-path 404', async ({ page }) => {
    await page.goto('/invalid/path?uiDemo=true&demo=true')

    await expect(page).toHaveURL(/\/\?(?=.*demo=true)(?=.*uiDemo=true)/)
    await expect(page.getByLabel('LAN pairing demo')).toBeVisible()
  })
})
