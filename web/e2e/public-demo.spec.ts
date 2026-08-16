import { expect, test } from '@playwright/test'

test.describe('public demo build', () => {
  test('forces the complete mock console, preserves scene state, and never reaches devd', async ({
    page,
  }) => {
    const controlRequests: string[] = []
    page.on('request', (request) => {
      if (/127\.0\.0\.1:30080|\/api\/v1\//.test(request.url())) controlRequests.push(request.url())
    })

    await page.goto('/?demo=false&uiDemo=true')
    await expect(page).toHaveURL(/\/devices\/fp-lab-01\/overview\?demo=true$/)
    await expect(page.getByRole('heading', { name: '热控工作台' })).toBeVisible()
    await expect(page.getByRole('complementary', { name: 'Demo Inspector' })).toBeVisible()
    await expect(page.getByLabel('LAN pairing demo')).toHaveCount(0)

    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()
    await page.getByRole('button', { name: 'Simulate thermal warning' }).click()
    await expect(page.getByText('simulated thermal warning acknowledged').last()).toBeVisible()

    await page.getByRole('button', { name: 'Degraded' }).click()
    await expect(page).toHaveURL(
      /\/devices\/fp-kit-02\/overview\?(?=.*demo=true)(?=.*demoScene=degraded)/
    )
    await expect(page.getByText('Simulate lease conflict')).toBeVisible()
    expect(controlRequests).toEqual([])
  })

  test('uses a mobile bubble that opens a touch-safe drawer', async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 812 })
    await page.goto('/')
    const openButton = page.getByRole('button', { name: '打开 Demo Inspector' })
    await expect(openButton).toBeVisible()
    expect((await openButton.boundingBox())?.height).toBeGreaterThanOrEqual(48)
    await openButton.click()
    await expect(page.getByRole('heading', { name: 'Demo Inspector' })).toBeVisible()
    const reset = page.getByRole('button', { name: 'Reset demo state' })
    expect((await reset.boundingBox())?.height).toBeGreaterThanOrEqual(48)
  })

  test('keeps the fixed desktop workstation at its natural width before the dock threshold', async ({
    page,
  }) => {
    await page.setViewportSize({ width: 1440, height: 1000 })
    await page.goto('/')

    await expect(page.getByRole('button', { name: '打开 Demo Inspector' })).toBeVisible()
    const layout = await page.evaluate(() => {
      const console = document.querySelector('.industrial-console')?.getBoundingClientRect()
      const wrap = document.querySelector('.industrial-console-wrap')
      return {
        consoleWidth: console?.width ?? 0,
        paddingRight: Number.parseFloat(getComputedStyle(wrap).paddingRight),
      }
    })

    expect(layout.consoleWidth).toBeGreaterThanOrEqual(1279)
    expect(layout.paddingRight).toBeLessThan(100)

    await page.setViewportSize({ width: 1700, height: 1000 })
    await page.goto('/')
    await expect(page.getByRole('heading', { name: 'Demo Inspector' })).toBeVisible()
    const dockedLayout = await page.evaluate(() => {
      const console = document.querySelector('.industrial-console')?.getBoundingClientRect()
      const wrap = document.querySelector('.industrial-console-wrap')
      return {
        consoleWidth: console?.width ?? 0,
        paddingRight: Number.parseFloat(getComputedStyle(wrap).paddingRight),
      }
    })

    expect(dockedLayout.consoleWidth).toBeGreaterThanOrEqual(1279)
    expect(dockedLayout.paddingRight).toBeGreaterThanOrEqual(380)
  })
})
