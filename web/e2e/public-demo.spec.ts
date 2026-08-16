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
})
