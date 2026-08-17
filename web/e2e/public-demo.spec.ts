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
    expect(await page.locator('script[src="/@vite/client"]').count()).toBe(0)

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

  test('serializes concurrent inspector changes into one share state', async ({ page }) => {
    await page.goto('/')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    await page.getByRole('checkbox', { name: 'Simulate lease conflict' }).evaluate((lease) => {
      const network = Array.from(
        document.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')
      ).find((input) => input.parentElement?.textContent?.includes('Simulate network timeout'))
      ;(lease as HTMLInputElement).click()
      network?.click()
    })

    await expect.poll(() => new URL(page.url()).searchParams.get('demoLease')).toBe('conflict')
    await expect.poll(() => new URL(page.url()).searchParams.get('demoNetwork')).toBe('timeout')
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

    for (const button of [
      page.getByRole('button', { name: '收起 Demo Inspector' }),
      page.getByRole('button', { name: '复制 Demo 分享链接' }),
    ]) {
      const box = await button.boundingBox()
      expect(box?.height).toBeGreaterThanOrEqual(48)
      expect(box?.width).toBeGreaterThanOrEqual(48)
    }
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
      const inspector = document.querySelector('.demo-inspector')?.getBoundingClientRect()
      const wrap = document.querySelector('.industrial-console-wrap')
      return {
        consoleWidth: console?.width ?? 0,
        consoleRight: console?.right ?? 0,
        inspectorLeft: inspector?.left ?? 0,
        paddingRight: Number.parseFloat(getComputedStyle(wrap).paddingRight),
      }
    })

    expect(dockedLayout.consoleWidth).toBeGreaterThanOrEqual(1279)
    expect(dockedLayout.paddingRight).toBeGreaterThanOrEqual(380)
    expect(dockedLayout.inspectorLeft - dockedLayout.consoleRight).toBeGreaterThanOrEqual(24)
  })

  test('uses English names for every demo target fixture', async ({ page }) => {
    await page.goto('/')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    await expect(page.getByRole('button', { name: /Bench Fixture A MOCK/ })).toBeVisible()
    await expect(page.getByRole('button', { name: /Field Kit SIMULATED SERIAL/ })).toBeVisible()
    await expect(page.getByRole('button', { name: /Offline Mock Device MOCK/ })).toBeVisible()
  })

  test('keeps mock targets usable and guards a calibration target switch', async ({ page }) => {
    await page.goto('/')
    await page.getByRole('button', { name: '打开 Demo Inspector' }).click()

    const fieldKit = page.getByRole('button', { name: /Field Kit SIMULATED SERIAL/ })
    await fieldKit.click()
    await expect(page).toHaveURL(/\/devices\/fp-kit-02\/overview\?demo=true$/)
    await expect(page.getByRole('heading', { name: '热控工作台' })).toBeVisible()
    await expect(page.getByRole('heading', { name: '目标设备暂不可用' })).toHaveCount(0)

    await page.getByRole('button', { name: 'Calibration Leave guard active' }).click()
    await expect(page).toHaveURL(
      /\/devices\/fp-lab-01\/calibration\/heater-curve\?(?=.*demo=true)(?=.*demoScene=calibration-active)/
    )
    await expect(page.getByRole('switch', { name: '标定模式' })).toBeChecked()

    await fieldKit.click()
    await expect(page.getByRole('dialog', { name: '校准未关闭' })).toBeVisible()
    await expect(page).toHaveURL(
      /\/devices\/fp-lab-01\/calibration\/heater-curve\?(?=.*demoScene=calibration-active)/
    )
    await page.getByRole('button', { name: '留在当前页' }).click()
    await expect(page.getByRole('dialog', { name: '校准未关闭' })).toHaveCount(0)

    await fieldKit.click()
    await page.getByRole('button', { name: '关闭并继续' }).click()
    await expect(page).toHaveURL(/\/devices\/fp-kit-02\/overview\?demo=true$/)
    await expect(page.getByRole('heading', { name: '热控工作台' })).toBeVisible()
    await expect(page.getByRole('heading', { name: '目标设备暂不可用' })).toHaveCount(0)
  })

  test('renders every public demo scene through its deterministic route', async ({ page }) => {
    const scenes = [
      {
        url: '/devices/fp-lab-01/overview?demo=true',
        visibleHeading: '热控工作台',
      },
      {
        url: '/devices/fp-kit-02/overview?demo=true&demoScene=degraded',
        visibleHeading: '热控工作台',
      },
      {
        url: '/devices/fp-demo-03/overview?demo=true&demoScene=offline',
        visibleHeading: '目标设备暂不可用',
      },
      {
        url: '/devices/fp-lab-01/overview?demo=true&demoScene=blocked-artifact',
        visibleHeading: '热控工作台',
      },
      {
        url: '/devices/fp-lab-01/calibration/heater-curve?demo=true&demoScene=calibration-active',
        visibleHeading: '热控工作台',
      },
    ]

    for (const scene of scenes) {
      await page.goto(scene.url)
      await expect(page.getByRole('heading', { name: scene.visibleHeading })).toBeVisible()
    }
  })
})
