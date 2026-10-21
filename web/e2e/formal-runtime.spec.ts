import { expect, test } from '@playwright/test'

test.describe('formal runtime boundary', () => {
  test('stops legacy demo query entries before mounting the live console', async ({ page }) => {
    const controlRequests: string[] = []
    page.on('request', (request) => {
      if (/127\.0\.0\.1:30080|\/api\/v1\//.test(request.url())) {
        controlRequests.push(request.url())
      }
    })

    await page.goto('/?demo=true')
    await expect(page.getByRole('heading', { name: '旧 Demo 入口已停用' })).toBeVisible()
    await expect(page.getByRole('heading', { name: '热控工作台' })).toHaveCount(0)
    await expect(page.getByRole('link', { name: '打开独立 Demo' })).toHaveAttribute(
      'href',
      'https://flux-purr-demo.ivanli.cc/'
    )

    await page.goto('/?uiDemo=firmware-workspace')
    await expect(page.getByRole('heading', { name: '旧 Demo 入口已停用' })).toBeVisible()
    await page.goto('/devices/fp-lab-01/overview?uiDemo=firmware-workspace')
    await expect(page.getByRole('heading', { name: '旧 Demo 入口已停用' })).toBeVisible()
    await page.goto('/devices/fp-lab-01/overview?demo=true')
    await expect(page.getByRole('heading', { name: '旧 Demo 入口已停用' })).toBeVisible()
    expect(controlRequests).toEqual([])
  })
})
